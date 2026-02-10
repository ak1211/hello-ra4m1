// hello-ra4m1
// Arduino UNO R4 MINIMA でSWDコネクタにあるシリアル通信を動作させる
//
// SPDX-License-Identifier: MIT
// SPDX-FileCopyrightText: 2026 Akihiro Yamamoto <github.com/ak1211>

#![no_std]
#![no_main]

use bbqueue::nicknames::Jerk;
use core::cell::Cell;
use cortex_m::interrupt::InterruptNumber;
use critical_section::Mutex;
use defmt;
use defmt_rtt as _;
use heapless::{String, Vec, format};
use panic_probe as _;
use ra4m1_fsp_pac as pac;
use ra4m1_fsp_pac::interrupt;
use scopeguard::defer;

// クロック設定
// 高速オンチップオシレータ(HOCO)を48MHzでメインクロックに設定する
fn clock_init_hoco48(p: &pac::Peripherals) {
    // 保護レジスタを操作して書込み許可を与える
    p.SYSTEM.prcr().write(|w| {
        w.prkey().set(0xa5);
        w.prc0().set_bit(); // クロック発生回路関連レジスタに書込み許可を与える
        w.prc1().set_bit() // 低消費電力モード関連レジスタに書込み許可を与える
    });
    // 関数脱出時に保護レジスタを元通りに復帰する
    defer! {
        p.SYSTEM.prcr().write(|w| {
            w.prkey().set(0xa5);
            w.prc0().clear_bit();
            w.prc1().clear_bit()
        });
    }

    // 消費電力モードはハイスピードモードに設定
    p.SYSTEM.opccr().write(|w| w.opcm()._00());
    while !p.SYSTEM.opccr().read().opcmtsf().bit_is_clear() {} // 確認

    // サブクロックの停止
    p.SYSTEM.sosccr().write(|w| w.sostp().set_bit()); // サブクロックの停止
    while !p.SYSTEM.sosccr().read().sostp().bit_is_set() {} // サブクロック停止確認

    // 高速オンチップオシレータ(HOCO)48MHz指定
    // HOCOCR2レジスタのアドレス: 0x4001_e037
    // HOCO48MHz指定: 0b0010_0000
    unsafe { core::ptr::write_volatile(0x4001_e037 as *mut u8, 0b0010_0000) };

    // 高速オンチップオシレータ(HOCO)クロック動作
    p.SYSTEM.hococr().write(|w| w.hcstp()._0());
    while !p.SYSTEM.hococr().read().hcstp().is_0() {} // 確認

    // 高速オンチップオシレータ(HOCO)クロック発振安定待ち
    while !p.SYSTEM.oscsf().read().hocosf().bit_is_set() {}

    // 分周器設定
    p.SYSTEM.sckdivcr().write(|w| {
        w.ick()._000(); // システムクロック(ICLK Div /1)
        w.pcka()._000(); // 周辺モジュールクロックA(PCLKA Div /1)
        w.pckb()._001(); // 周辺モジュールクロックB(PCLKB Div /2)
        w.pckc()._000(); // 周辺モジュールクロックC(PCLKC Div /1)
        w.pckd()._000(); // 周辺モジュールクロックD(PCLKD Div /1)
        w.fck()._001() // Flashインターフェースクロック(FCLK Div /2)
    });

    // システムクロックを高速オンチップオシレータ(HOCO)クロックに切り替え
    p.SYSTEM.sckscr().write(|w| w.cksel()._000()); // HOCOクロック
    while !p.SYSTEM.sckscr().read().cksel().is_000() {} // 確認

    // フラッシュキャッシュ
    p.FCACHE.fcacheiv().write(|w| w.fcacheiv()._1()); // フラッシュキャッシュインバリデート
    while !p.FCACHE.fcacheiv().read().fcacheiv().is_0() {} // 確認

    p.FCACHE.fcachee().write(|w| w.fcacheen().set_bit()); // フラッシュキャッシュ許可
}

// GPTタイマーモジュール設定
fn gpt_module_init(p: &pac::Peripherals) {
    // GPT321~GPT320モジュールのモジュールストップ状態の解除
    p.MSTP.mstpcrd().modify(|_r, w| w.mstpd5()._0());

    // GPT320タイマーカウント動作を停止
    p.GPT320.gtcr().modify(|_r, w| w.cst()._0());

    // UPカウント設定
    p.GPT320.gtuddtyc().modify(|_r, w| w.ud()._1());

    // カウンタ最大値設定
    let period_count: u32 = 48_000_000; // 1秒周期
    p.GPT320
        .gtpr()
        .write(|w| unsafe { w.bits(period_count - 1) });

    // カウンタ初期値設定
    p.GPT320.gtcnt().reset();

    // GPT320 オーバーフロー割り込み設定
    p.ICU
        .ielsr(GPT320_OVERFLOW_IEL.number() as usize)
        .modify(|_r, w| w.iels().set(0x05d));

    // GPT320 タイマーモジュール割り込み有効
    unsafe { cortex_m::peripheral::NVIC::unmask(GPT320_OVERFLOW_IEL) };
}

// GPT320タイマオーバーフロー検出フラグ
static GPT320_TIMER_OVERFLOW_FLAG: Mutex<Cell<bool>> = Mutex::new(Cell::new(false));

// タイマオーバーフロー割り込み番号
const GPT320_OVERFLOW_IEL: pac::Interrupt = pac::Interrupt::IEL10;

// タイマオーバーフロー割り込みハンドラ
#[cortex_m_rt::interrupt]
fn IEL10() {
    let p = unsafe { pac::Peripherals::steal() };

    if p.GPT320.gtst().read().tcfpo().is_1() {
        // タイマオーバーフロー割り込み
        critical_section::with(|cs| GPT320_TIMER_OVERFLOW_FLAG.borrow(cs).replace(true));
        // タイマオーバーフロー割り込みフラグクリア
        p.GPT320.gtst().modify(|_r, w| w.tcfpo().clear_bit());
    }

    // 割り込みステータスフラグクリア
    p.ICU.ielsr(10).modify(|_r, w| w.ir().clear_bit());
}

const QUEUE_SIZE: usize = 64;

// シリアル通信受信待ち行列
static RXD_QUEUE: Jerk<QUEUE_SIZE> = Jerk::new();

// シリアル通信送信待ち行列
static TXD_QUEUE: Jerk<QUEUE_SIZE> = Jerk::new();

// シリアルコミュニケーションインタフェース(SCI)モジュール設定
fn sci_module_init(p: &pac::Peripherals) {
    // SCI1モジュールのモジュールストップ状態の解除
    p.MSTP.mstpcrb().modify(|_r, w| w.mstpb30()._0());

    // SCI動作を停止
    p.SCI1.scr().reset();

    // FIFO動作を禁止
    p.SCI1.fcr().modify(|_r, w| w.fm()._0());

    // 内蔵ボーレートジェネレータを選択
    p.SCI1.scr().modify(|_r, w| w.cke()._00());

    // 調歩同期
    p.SCI1.simr1().modify(|_r, w| w.iicm()._0());

    //
    p.SCI1.spmr().modify(|_r, w| {
        w.sse()._0(); // SSn端子機能は無効 
        w.ctse()._0(); // CTS機能は無効（RTS出力機能は有効）
        w.mss()._0(); // TXDn端子は送信、RXDn端子は受信（マスタモード）
        w.mff()._0(); // モードフォルトエラーなし
        w.ckpol()._0(); // クロック極性反転なし
        w.ckph()._0() // クロック遅延なし
    });

    //
    p.SCI1.scmr().modify(|_r, w| {
        w.smif()._0(); // 非スマートカードインタフェースモード
        w.sinv()._0(); // TDRレジスタの内容をそのまま送信。受信データをそのままRDRレジスタに格納
        w.sdir()._0(); // LSBファースト転送
        w.chr1()._1() // データ長8ビットで送受信
    });

    //
    p.SCI1.smr().modify(|_r, w| {
        w.cks()._00(); // PCLKA /1 クロック (n = 0)
        w.mp()._0(); // マルチプロセッサ通信機能は無効
        w.stop()._0(); // STOP: 1bit
        w.pe()._0(); // パリティビットを付加しない
        w.chr()._0(); // データ長8ビットで送受信
        w.cm()._0() // 調歩同期式モード
    });

    //
    p.SCI1.semr().modify(|_r, w| {
        w.bgdm()._0(); // ボーレートジェネレータから1倍の周波数のクロックを出力
        w.brme()._0();
        w.abcs()._0(); // 基本クロックの16サイクルを1ビット期間として選択
        w.abcse()._0() // 1ビット期間のクロックサイクルは、SEMRレジスタのBGDMとABCS の組み合わせにより決定
    });

    // PCLKA = 48MHz
    // n = 0
    // B = 115200 bps
    // 2^(2n-1) = 2^(-1) = 1/2

    //       48 * 10^6
    // N = --------------------- - 1 = 13 - 1 = 12
    //       64 * 1/2 * 115200

    p.SCI1.brr().write(|w| unsafe { w.bits(12) });

    // イベント番号
    const SCI1_RXI_EVENT_NUMBER: u8 = 0x09e;
    const SCI1_TXI_EVENT_NUMBER: u8 = 0x09f;
    const SCI1_TEI_EVENT_NUMBER: u8 = 0x0a0;
    const SCI1_ERI_EVENT_NUMBER: u8 = 0x0a1;

    // シリアル通信受信データ割り込み設定
    p.ICU
        .ielsr(SCI1_RXI_IEL.number() as usize)
        .modify(|_r, w| w.iels().set(SCI1_RXI_EVENT_NUMBER));

    // シリアル通信送信データエンプティ割り込み設定
    p.ICU
        .ielsr(SCI1_TXI_IEL.number() as usize)
        .modify(|_r, w| w.iels().set(SCI1_TXI_EVENT_NUMBER));

    // シリアル通信送信終了割り込み設定
    p.ICU
        .ielsr(SCI1_TEI_IEL.number() as usize)
        .modify(|_r, w| w.iels().set(SCI1_TEI_EVENT_NUMBER));

    // シリアル通信エラー割り込み設定
    p.ICU
        .ielsr(SCI1_ERI_IEL.number() as usize)
        .modify(|_r, w| w.iels().set(SCI1_ERI_EVENT_NUMBER));

    // SCI1モジュール割り込み有効
    unsafe {
        cortex_m::peripheral::NVIC::unmask(SCI1_RXI_IEL);
        cortex_m::peripheral::NVIC::unmask(SCI1_TXI_IEL);
        cortex_m::peripheral::NVIC::unmask(SCI1_TEI_IEL);
        cortex_m::peripheral::NVIC::unmask(SCI1_ERI_IEL);
    }

    // I/Oポートの設定
    let _ = {
        // 書き込みプロテクトレジスタを操作してPmnPFS レジスタに書き込み許可を与える
        p.PMISC.pwpr().write(|w| w.b0wi()._0());
        p.PMISC.pwpr().write(|w| w.pfswe()._1());

        // 離脱時に書き込みプロテクトレジスタを元通りに復帰する
        defer! {
        p.PMISC.pwpr().write(|w| w.pfswe()._0());
        p.PMISC.pwpr().write(|w| w.b0wi()._1());
        }

        if false {
            // PORT 012 = TX_LED
            p.PFS
                .p012pfs()
                .modify(|_r, w| w.pcr()._0().pdr()._1().podr()._1().ncodr()._0());
            // PORT 013 = RX_LED
            p.PFS
                .p013pfs()
                .modify(|_r, w| w.pcr()._0().pdr()._1().podr()._1().ncodr()._0());
            // PORT 501 = SCI1_TXD
            p.PFS.p501pfs().reset();
            p.PFS.p501pfs().modify(|_r, w| {
                unsafe { w.psel().bits(0b00101) };
                w.pcr()._0().pdr()._1().ncodr()._0()
            });
            p.PFS.p502pfs().modify(|_r, w| w.pmr()._1());
            // PORT 502 = SCI1_RXD
            p.PFS.p502pfs().reset();
            p.PFS.p502pfs().modify(|_r, w| {
                unsafe { w.psel().bits(0b00101) };
                w.pcr()._0().pdr()._0().ncodr()._0()
            });
            p.PFS.p502pfs().modify(|_r, w| w.pmr()._1());
        } else {
            //
            // このあたりアドレスが変なので、ユーザーズマニュアルの値でwrite_volatileしてみる
            //
            let ptr = p.PFS.p110pfs().as_ptr();
            defmt::debug!("P110pfs is {:X}", ptr);
            let ptr = p.PFS.p111pfs().as_ptr();
            defmt::debug!("P111pfs is {:X}", ptr);
            let ptr = p.PFS.p112pfs().as_ptr();
            defmt::debug!("P112pfs is {:X}", ptr);
            let ptr = p.PFS.p113pfs().as_ptr();
            defmt::debug!("P113pfs is {:X}", ptr);
            let ptr = p.PFS.p114pfs().as_ptr();
            defmt::debug!("P114pfs is {:X}", ptr);
            let ptr = p.PFS.p115pfs().as_ptr();
            defmt::debug!("P115pfs is {:X}", ptr);
            let ptr = p.PFS.p010pfs().as_ptr();
            defmt::debug!("P010pfs is {:X}", ptr);
            let ptr = p.PFS.p011pfs().as_ptr();
            defmt::debug!("P011pfs is {:X}", ptr);
            let ptr = p.PFS.p012pfs().as_ptr();
            defmt::debug!("P012pfs is {:X}", ptr);
            let ptr = p.PFS.p013pfs().as_ptr();
            defmt::debug!("P013pfs is {:X}", ptr);
            let ptr = p.PFS.p014pfs().as_ptr();
            defmt::debug!("P014pfs is {:X}", ptr);
            let ptr = p.PFS.p015pfs().as_ptr();
            defmt::debug!("P015pfs is {:X}", ptr);
            let ptr = p.PFS.p500pfs().as_ptr();
            defmt::debug!("P500pfs is {:X}", ptr);
            let ptr = p.PFS.p501pfs().as_ptr();
            defmt::debug!("P501pfs is {:X}", ptr);
            let ptr = p.PFS.p502pfs().as_ptr();
            defmt::debug!("P502pfs is {:X}", ptr);
            let ptr = p.PFS.p503pfs().as_ptr();
            defmt::debug!("P503pfs is {:X}", ptr);
            let ptr = p.PFS.p504pfs().as_ptr();
            defmt::debug!("P504pfs is {:X}", ptr);
            let ptr = p.PFS.p505pfs().as_ptr();
            defmt::debug!("P505pfs is {:X}", ptr);
            //
            unsafe {
                // P010PFS  = 0x4004_0828
                // P011PFS  = 0x4004_0828 +  4 = 0x4004_082c
                // P012PFS  = 0x4004_0828 +  8 = 0x4004_0830
                // P013PFS  = 0x4004_0828 + 12 = 0x4004_0834
                // P014PFS  = 0x4004_0828 + 16 = 0x4004_0838
                // P015PFS  = 0x4004_0828 + 16 = 0x4004_083c
                const P012PFS_ADDR: *mut u32 = 0x4004_0830 as *mut u32;
                const P013PFS_ADDR: *mut u32 = 0x4004_0834 as *mut u32;
                let podr_bit: u32 = 1 << 0;
                let pdr_bit: u32 = 1 << 2;
                // PORT 012 = TX_LED
                core::ptr::write_volatile(P012PFS_ADDR, pdr_bit | podr_bit);
                // PORT 013 = RX_LED
                core::ptr::write_volatile(P013PFS_ADDR, pdr_bit | podr_bit);
                // P500PFS  = 0x4004_0940
                // P501PFS  = 0x4004_0940 +  4 = 0x4004_0944
                // P502PFS  = 0x4004_0940 +  8 = 0x4004_0948
                // P503PFS  = 0x4004_0940 + 12 = 0x4004_094c
                // P504PFS  = 0x4004_0940 + 16 = 0x4004_0950
                // P505PFS  = 0x4004_0940 + 20 = 0x4004_0954
                const P501PFS_ADDR: *mut u32 = 0x4004_0944 as *mut u32;
                const P502PFS_ADDR: *mut u32 = 0x4004_0948 as *mut u32;
                let psel_bit: u32 = 0b00101 << 24;
                let pmr_bit: u32 = 1 << 16;
                let pdr_bit: u32 = 1 << 2;
                // PORT 501 = SCI1_TXD
                core::ptr::write_volatile(P501PFS_ADDR, psel_bit | pmr_bit | pdr_bit);
                // PORT 502 = SCI1_RXD
                core::ptr::write_volatile(P502PFS_ADDR, psel_bit | pmr_bit);
            }
        };
    };

    // シリアル送信が動作していない時は1を出力
    p.SCI1.sptr().write(|w| w.spb2dt()._1().spb2io()._1());

    // エラーステータスフラグクリア
    p.SCI1
        .ssr()
        .modify(|_r, w| w.per()._0().fer()._0().orer()._0());

    //
    p.SCI1.scr().modify(|_r, w| {
        w.rie()._1(); // SCIn_RXI割り込み要求を許可
        w.tie()._0(); // SCIn_TXI割り込み要求を禁止
        w.teie()._0(); // SCIn_TEI割り込み要求を禁止
        w.re()._1(); // シリアル受信動作を許可
        w.te()._0() // シリアル送信動作を禁止
    });
}

// シリアル通信受信データ割り込み番号
const SCI1_RXI_IEL: pac::Interrupt = pac::Interrupt::IEL6;

// シリアル通信受信データ割り込みハンドラ
#[cortex_m_rt::interrupt]
fn IEL6() {
    let p = unsafe { pac::Peripherals::steal() };
    // RX_LED (PORT 013) を点灯
    p.PORT0
        .podr()
        .modify(|r, w| unsafe { w.bits(r.bits() & !(1 << 13)) });

    //
    let rxd_prod = RXD_QUEUE.stream_producer();
    // 受信データーをシリアル受信待ち行列に追加する
    let mut wgrant = rxd_prod.grant_exact(1).unwrap();
    wgrant[0] = p.SCI1.rdr().read().bits();
    wgrant.commit(1);

    // RX_LED (PORT 013) を消灯
    p.PORT0
        .podr()
        .modify(|r, w| unsafe { w.bits(r.bits() | (1 << 13)) });

    // 割り込みステータスフラグクリア
    p.ICU.ielsr(6).modify(|_r, w| w.ir().clear_bit());
}

// シリアル送信バッファに送る
fn uart_println(input: &[u8]) {
    let txd_prod = TXD_QUEUE.stream_producer();
    let mut wgrant = txd_prod.grant_exact(input.len() + 2).unwrap();

    wgrant[0..input.len()].copy_from_slice(input);
    wgrant[input.len()..].copy_from_slice(b"\r\n");
    wgrant.commit(input.len() + 2);

    //
    let p = unsafe { pac::Peripherals::steal() };

    // シリアル送信動作を許可
    p.SCI1.scr().modify(|_r, w| {
        w.tie()._1(); // SCIn_TXI割り込み要求を許可
        w.teie()._0(); // SCIn_TEI割り込み要求を禁止
        w.te()._1() // シリアル送信動作を許可
    });
}

// シリアル通信送信データエンプティ割り込み番号
const SCI1_TXI_IEL: pac::Interrupt = pac::Interrupt::IEL7;

// シリアル通信送信データエンプティ割り込みハンドラ
#[cortex_m_rt::interrupt]
fn IEL7() {
    let p = unsafe { pac::Peripherals::steal() };

    let txd_cons = TXD_QUEUE.stream_consumer();

    // 送信
    if let Ok(rgr) = txd_cons.read() {
        let txd = rgr[0];
        rgr.release(1);

        // TX_LED (PORT 012) を点灯
        p.PORT0
            .podr()
            .modify(|r, w| unsafe { w.bits(r.bits() & !(1 << 12)) });

        p.SCI1.tdr().write(|w| unsafe { w.bits(txd) });

        //
        p.SCI1.scr().modify(|_r, w| {
            w.tie()._1(); // SCIn_TXI割り込み要求を許可
            w.teie()._0() // SCIn_TEI割り込み要求を禁止
        });
    } else {
        p.SCI1.scr().modify(|_r, w| {
            w.tie()._0(); // SCIn_TXI割り込み要求を禁止
            w.teie()._1() // SCIn_TEI割り込み要求を許可
        });
    }

    // 割り込みステータスフラグクリア
    p.ICU.ielsr(7).modify(|_r, w| w.ir().clear_bit());
}

// シリアル通信送信終了割り込み番号
const SCI1_TEI_IEL: pac::Interrupt = pac::Interrupt::IEL8;

// シリアル通信送信終了割り込みハンドラ
#[cortex_m_rt::interrupt]
fn IEL8() {
    let p = unsafe { pac::Peripherals::steal() };

    // シリアル送信動作を禁止
    p.SCI1.scr().modify(|_r, w| {
        w.tie()._0(); // SCIn_TXI割り込み要求を禁止
        w.teie()._0(); // SCIn_TEI割り込み要求を禁止
        w.te()._0() // シリアル送信動作を禁止
    });

    // TX_LED (PORT 012) を消灯
    p.PORT0
        .podr()
        .modify(|r, w| unsafe { w.bits(r.bits() | (1 << 12)) });

    // 割り込みステータスフラグクリア
    p.ICU.ielsr(8).modify(|_r, w| w.ir().clear_bit());
}

// シリアル通信エラー割り込み番号
const SCI1_ERI_IEL: pac::Interrupt = pac::Interrupt::IEL9;

// シリアル通信エラー割り込みハンドラ
#[cortex_m_rt::interrupt]
fn IEL9() {
    let p = unsafe { pac::Peripherals::steal() };

    // シリアル通信エラーステータス
    let ssr = p.SCI1.ssr().read().bits();
    defmt::error!("{:X}", ssr);

    // シリアル通信エラーステータスフラグクリア
    p.SCI1
        .ssr()
        .modify(|_r, w| w.per()._0().fer()._0().orer()._0());

    // 割り込みステータスフラグクリア
    p.ICU.ielsr(9).modify(|_r, w| w.ir().clear_bit());
}

// ADCモジュール設定
fn adc_module_init(p: &pac::Peripherals) {
    // ADC14モジュールのモジュールストップ状態の解除
    p.MSTP.mstpcrd().modify(|_r, w| w.mstpd16()._0());

    // A/D変換を停止する
    p.ADC140.adcsr().modify(|_r, w| w.adst()._0());
    p.ADC140.adansa0().reset();
    p.ADC140.adansa1().reset();
    p.ADC140.adansb0().reset();
    p.ADC140.adansb1().reset();

    // A/D変換設定
    p.ADC140.adcer().modify(|_r, w| {
        w.adprc()._11(); // 14ビット精度
        w.adrfmt()._0() // A/Dデータレジスタのフォーマットを右詰めにする
    });

    // サンプリング時間設定
    // 周辺モジュールクロックC(PCLKC Div /1) = 48MHz
    p.ADC140
        .adsstrt()
        .modify(|_r, w| unsafe { w.sst().bits(100) });

    // 内部ノードディスチャージ（基準電圧端子を選択しない）
    p.ADC140.adhvrefcnt().modify(|_r, w| {
        w.hvsel()._11(); // 内部ノードディスチャージ（基準電圧端子を選択しない）
        w.adslp()._0() // 通常動作
    });
    cortex_m::asm::nop();
    cortex_m::asm::nop();
    cortex_m::asm::nop();
    cortex_m::asm::nop();
    cortex_m::asm::nop();

    // 高電位基準電圧にAVCC0を選択
    // Arduino UNO R4 MINIMAの場合 5V
    p.ADC140.adhvrefcnt().modify(|_r, w| {
        w.hvsel()._00() // 高電位基準電圧にAVCC0を選択
    });
    cortex_m::asm::nop();
    cortex_m::asm::nop();
    cortex_m::asm::nop();
    cortex_m::asm::nop();
    cortex_m::asm::nop();
}

// 内蔵温度センサの値を読み取る
fn read_tsn(p: &pac::Peripherals) -> f32 {
    // ユーザーズマニュアルにおける TSNの章より計算式
    //
    // 温度（T）はセンサの電圧出力（Vs）と比例関係にあるため、以下の式で温度を求められます。
    // T = (Vs - V1) / Slope + T1
    // T：測定温度（℃）
    // Vs：温度測定時の温度センサの出力電圧（V）
    // T1：1 点目の試行測定時の温度（℃）
    // V1：T1 測定時の温度センサの出力電圧（V）
    // T2：2 点目の試行測定時の温度（℃）
    // V2：T2 測定時の温度センサの出力電圧（V）
    // Slope：温度センサの温度傾斜（V/ ℃）、Slope = (V2 - V1) / (T2 - T1)

    // A/D変換を停止する
    p.ADC140.adcsr().modify(|_r, w| w.adst()._0());
    p.ADC140.adexicr().modify(|_r, w| {
        w.ocsa()._0(); // 内部基準電圧のA/D変換禁止
        w.tssad()._0(); // 温度センサ出力A/D変換値加算／平均モード非選択
        w.tssa()._1() // 温度センサ出力のA/D変換許可
    });

    // シングルスキャンモードでA/D変換開始
    p.ADC140.adcsr().modify(|_r, w| {
        w.adcs()._00(); // シングルスキャンモード
        w.adst()._1() // A/D変換開始
    });

    // Ta = Tj = 125 ℃および AVCC0 = 3.3V の条件で、
    // 温度センサが出力した電圧を、
    // FFh を書き込む ADC16 によって変換した温度センサの温度値（CAL125）
    // 4096は2の12乗
    let cal125 = {
        let h = p.TSN.tscdrh().read().bits() as u16; // 上位4ビット
        let l = p.TSN.tscdrl().read().bits() as u16; // 下位8ビット
        ((h << 8) + l) & (4096 - 1)
    };

    // V1：T1 測定時の温度センサの出力電圧（V）
    let v1 = 3.3 * (cal125 as f32) / 4096.0;

    // A/D変換待ち
    while p.ADC140.adcsr().read().adst().is_1() {}

    // A/D 温度センサデータレジスタの値を読み取る
    // 14ビット右詰め値
    // 16384は2の14乗
    let tsn = p.ADC140.adtsdr().read().bits() & (16384 - 1);

    // Vs：温度測定時の温度センサの出力電圧（V）
    let vs = 5.0 * (tsn as f32) / 16384.0;

    // ユーザーズマニュアル(TSN 特性)より温度傾斜
    const SLOPE: f32 = -3.65 / 1000.0; // V/℃

    // 内蔵温度センサの値
    (vs - v1) / SLOPE + 125.0 // ℃
}

#[cortex_m_rt::entry]
fn main() -> ! {
    let _ = {
        // ファクトリ MCU インフォメーションフラッシュルートテーブル (FMIFRT)
        const FMIFRT: *const u32 = 0x407f_b19c as *const u32;

        // ユニークIDのベースアドレス
        let unique_id_base_address = unsafe { core::ptr::read_volatile(FMIFRT) } as *const u32;

        //
        let mut buf: Vec<u8, 16> = Vec::new();

        // 型名レジスタ n（PNRn）（n = 0 ～ 3）
        // ユニークIDのベースアドレスに対するオフセットは 24h, 28h, 2ch, 30h
        for offset in [0x24, 0x28, 0x2c, 0x30] {
            let pnr: u32 = unsafe {
                core::ptr::read_volatile(unique_id_base_address.wrapping_byte_add(offset))
            };
            // バイトオーダーを変換
            let bytes = pnr.to_ne_bytes();
            //
            for i in 0..4 {
                buf.push(bytes[i]).unwrap();
            }
        }

        // heapless::Stringに変換
        let product_part_number: String<16> = String::from_utf8(buf).unwrap();

        // 挨拶
        defmt::info!(r#"Hello. I'm "{}""#, product_part_number.as_str());
    };

    // 周辺機能
    let p = pac::Peripherals::take().unwrap();

    let _ = {
        // 書き込みプロテクトレジスタを操作してPmnPFS レジスタに書き込み許可を与える
        p.PMISC.pwpr().write(|w| w.b0wi()._0());
        p.PMISC.pwpr().write(|w| w.pfswe()._1());

        // 離脱時に書き込みプロテクトレジスタを元通りに復帰する
        defer! {
        p.PMISC.pwpr().write(|w| w.pfswe()._0());
        p.PMISC.pwpr().write(|w| w.b0wi()._1());
        }

        // PORT 111 = D13(LED)
        // 以上の入出力ポートを出力に設定
        p.PFS.p111pfs().reset();
        p.PFS.p111pfs().modify(|_r, w| w.pdr()._1());
    };

    // ADCモジュール設定
    adc_module_init(&p);

    // 48MHzクロック設定
    clock_init_hoco48(&p);

    // GPTタイマーモジュールの設定
    gpt_module_init(&p);

    // SCIモジュールの設定
    sci_module_init(&p);

    // GPT320タイマーカウント動作を開始
    p.GPT320.gtcr().modify(|_r, w| {
        w.cst()._1();
        w.md()._000(); // のこぎり波形PWMモード
        w.tpcs()._000() // プリスケーラ― (PCLKD/1)
    });

    //
    // メインループ
    //
    let rxd_cons = RXD_QUEUE.stream_consumer();
    loop {
        // タイマー割り込みがあったか？
        let flag =
            critical_section::with(|cs| GPT320_TIMER_OVERFLOW_FLAG.borrow(cs).replace(false));
        // タイマー割り込みがあれば
        if flag {
            // 内蔵温度センサーの値を読む
            let t = read_tsn(&p);
            // 内蔵温度センサーの値をシリアル通信で出力する
            let _ = format!("{:>8.04} C", t).map(|s: String<20>| uart_println(s.as_bytes()));
            //
            if let Ok(rgr) = rxd_cons.read() {
                // シリアル通信でデーターを受信した
                let len = rgr.len();
                let text: String<QUEUE_SIZE> = rgr.iter().map(|&u| u as char).collect();
                defmt::info!("RXD: {}", text.as_str());
                rgr.release(len);
            }
        }
    }
}
