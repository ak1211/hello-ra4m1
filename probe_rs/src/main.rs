// hello-ra4m1
// Arduino UNO R4 MINIMA でLチカする
//
// SPDX-License-Identifier: MIT
// SPDX-FileCopyrightText: 2026 Akihiro Yamamoto <github.com/ak1211>

#![no_std]
#![no_main]

use cortex_m::delay::Delay;
use defmt;
use defmt_rtt as _;
use heapless::{String, Vec};
use panic_probe as _;
use ra4m1_fsp_pac as pac;
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
    unsafe { core::ptr::write_volatile(0x4001_e037 as *mut u8, 0b0010_0000u8) };

    // 高速オンチップオシレータ(HOCO)クロック動作
    p.SYSTEM.hococr().write(|w| w.hcstp()._0());
    while !p.SYSTEM.hococr().read().hcstp().is_0() {} // 確認

    // 高速オンチップオシレータ(HOCO)クロック発振安定待ち
    while !p.SYSTEM.oscsf().read().hocosf().bit_is_set() {}

    // 分周器設定
    p.SYSTEM.sckdivcr().write(|w| {
        w.ick()._000(); // システムクロック(ICLK Div /1)
        w.pcka()._000(); // 周辺モジュールクロックA(PCLKA Div /1)
        w.pckb()._001(); // 周辺モジュールクロックB(PCLKA Div /2)
        w.pckc()._000(); // 周辺モジュールクロックC(PCLKA Div /1)
        w.pckd()._000(); // 周辺モジュールクロックD(PCLKA Div /1)
        w.fck()._001() // Flashインターフェースクロック(FCLK Div /2)
    });

    // システムクロックを高速オンチップオシレータ(HOCO)クロックに切り替え
    p.SYSTEM.sckscr().write(|w| w.cksel()._000()); // HOCOクロック
    while !p.SYSTEM.sckscr().read().cksel().is_000() {} // 確認

    // フラッシュキャッシュ
    p.FCACHE.fcacheiv().write(|w| w.fcacheiv()._1()); // フラッシュキャッシュインバリデート

    while p.FCACHE.fcacheiv().read().fcacheiv().bit_is_set() {}
    p.FCACHE.fcachee().write(|w| w.fcacheen().set_bit()); // フラッシュキャッシュ許可
}

#[cortex_m_rt::entry]
fn main() -> ! {
    // 型名
    let product_part_number: String<16> = {
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
        String::from_utf8(buf).unwrap()
    };

    // 挨拶
    defmt::info!(r#"Hello. I'm "{}""#, product_part_number.as_str());

    // 周辺機能
    let p = pac::Peripherals::take().unwrap();
    let cp = cortex_m::Peripherals::take().unwrap();
    let mut delay = Delay::new(cp.SYST, 48_000_000);

    // クロック設定
    clock_init_hoco48(&p);

    //
    const LED: u16 = 1 << 11;

    // 書き込みプロテクトレジスタを操作して書込み許可を与える
    p.PMISC.pwpr().write(|w| w.b0wi()._0());
    p.PMISC.pwpr().write(|w| w.pfswe()._1());

    // PORT 111 = D13(LED) の入出力ポートを出力に設定
    p.PFS
        .p111pfs()
        .modify(|_r, w| w.pcr()._0().pdr()._1().ncodr()._0().pmr()._0());

    // 書き込みプロテクトレジスタを復帰する
    p.PMISC.pwpr().write(|w| w.pfswe()._0());
    p.PMISC.pwpr().write(|w| w.b0wi()._1());

    // メインループ
    loop {
        p.PORT1
            .podr()
            .modify(|r, w| unsafe { w.bits(r.bits() | LED) });
        delay.delay_ms(1000);
        p.PORT1
            .podr()
            .modify(|r, w| unsafe { w.bits(r.bits() & !LED) });
        delay.delay_ms(1000);
    }
}
