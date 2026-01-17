// hello-ra4m1
// RA4M1-Zero Mini Development Boardに乗っているWS2812BでLチカする
//
// SPDX-License-Identifier: MIT
// SPDX-FileCopyrightText: 2025 Akihiro Yamamoto <github.com/ak1211>

#![no_std]
#![no_main]

use core::sync::atomic::{AtomicBool, Ordering};
use cortex_m::delay::Delay;
use cortex_m::interrupt::InterruptNumber;
use panic_halt as _;
use ra4m1_fsp_pac as pac;
use ra4m1_fsp_pac::interrupt;
use scopeguard::defer;

// クロック設定
// 16MHz水晶発振子をメインクロックに設定する
#[allow(dead_code)]
fn clock_init_xtal(p: &pac::Peripherals) {
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

    // メインクロック発振器(MOSC)の停止
    p.SYSTEM.mosccr().write(|w| w.mostp()._1());
    while !p.SYSTEM.mosccr().read().mostp().is_1() {} // 確認

    // メインクロック発振器(MOSC)モードコントロールレジスタ
    p.SYSTEM.momcr().write(|w| {
        w.modrv1()._0(); // 10MHz ～ 20MHz
        w.mosel()._0() // 外部水晶発振子
    });

    // メインクロック発振器(MOSC)待機時間
    p.SYSTEM.moscwtcr().write(|w| w.msts()._1001()); // 32768us

    // メインクロック発振器(MOSC)動作
    p.SYSTEM.mosccr().write(|w| w.mostp()._0());
    while !p.SYSTEM.mosccr().read().mostp().is_0() {} // 確認

    // メインクロック発振器(MOSC)発振安定待ち
    while !p.SYSTEM.oscsf().read().moscsf().bit_is_set() {}

    // 分周器設定
    p.SYSTEM.sckdivcr().write(|w| {
        w.ick()._000(); // システムクロック(ICLK Div /1)
        w.pcka()._000(); // 周辺モジュールクロックA(PCLKA Div /1)
        w.pckb()._000(); // 周辺モジュールクロックB(PCLKA Div /1)
        w.pckc()._000(); // 周辺モジュールクロックC(PCLKA Div /1)
        w.pckd()._000(); // 周辺モジュールクロックD(PCLKA Div /1)
        w.fck()._000() // Flashインターフェースクロック(FCLK Div /1)
    });

    // システムクロックをメインクロックに切り替え
    p.SYSTEM.sckscr().write(|w| w.cksel()._011()); // メインクロック発振器(MOSC)
    while !p.SYSTEM.sckscr().read().cksel().is_011() {} // 確認

    // フラッシュキャッシュ
    p.FCACHE.fcacheiv().write(|w| w.fcacheiv()._1()); // フラッシュキャッシュインバリデート

    while p.FCACHE.fcacheiv().read().fcacheiv().bit_is_set() {}
    p.FCACHE.fcachee().write(|w| w.fcacheen().set_bit()); // フラッシュキャッシュ許可
}

// クロック設定
// 16MHz水晶発振子を12逓倍のち4分周した48MHzをクロックに設定する
#[allow(dead_code)]
fn clock_init_pll48(p: &pac::Peripherals) {
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

    // メインクロック発振器(MOSC)の停止
    p.SYSTEM.mosccr().write(|w| w.mostp()._1());
    while !p.SYSTEM.mosccr().read().mostp().is_1() {} // 確認

    //
    // メインクロック発振器(MOSC)の入力は16MHz水晶発振子
    //

    // メインクロック発振器(MOSC)モードコントロールレジスタ
    p.SYSTEM.momcr().write(|w| {
        w.modrv1()._0(); // 10MHz ～ 20MHz
        w.mosel()._0() // 外部水晶発振子
    });

    // メインクロック発振器(MOSC)待機時間
    p.SYSTEM.moscwtcr().write(|w| w.msts()._1001()); // 32768us

    // メインクロック発振器(MOSC)動作
    p.SYSTEM.mosccr().write(|w| w.mostp()._0());
    while !p.SYSTEM.mosccr().read().mostp().is_0() {} // 確認

    // メインクロック発振器(MOSC)発振安定待ち
    while !p.SYSTEM.oscsf().read().moscsf().bit_is_set() {}

    // メインクロック発振器(MOSC)をPLLで逓倍する
    // 逓倍率および分周比の設定
    p.SYSTEM.pllccr2().write(|w| {
        w.pllmul().set(12 - 1); // PLL Mul x12
        w.plodiv()._10() // PLL Div /4
    });

    // PLL動作
    p.SYSTEM.pllcr().write(|w| w.pllstp()._0());
    while !p.SYSTEM.pllcr().read().pllstp().is_0() {} // 確認

    // PLL発振安定待ち
    while !p.SYSTEM.oscsf().read().pllsf().bit_is_set() {}

    // 分周器設定
    p.SYSTEM.sckdivcr().write(|w| {
        w.ick()._000(); // システムクロック(ICLK Div /1)
        w.pcka()._000(); // 周辺モジュールクロックA(PCLKA Div /1)
        w.pckb()._001(); // 周辺モジュールクロックB(PCLKA Div /2)
        w.pckc()._000(); // 周辺モジュールクロックC(PCLKA Div /1)
        w.pckd()._000(); // 周辺モジュールクロックD(PCLKA Div /1)
        w.fck()._001() // Flashインターフェースクロック(FCLK Div /2)
    });

    // システムクロックをPLLに切り替え
    p.SYSTEM.sckscr().write(|w| w.cksel()._101()); // PLL
    while !p.SYSTEM.sckscr().read().cksel().is_101() {} // 確認

    // フラッシュキャッシュ
    p.FCACHE.fcacheiv().write(|w| w.fcacheiv()._1()); // フラッシュキャッシュインバリデート

    while p.FCACHE.fcacheiv().read().fcacheiv().bit_is_set() {}
    p.FCACHE.fcachee().write(|w| w.fcacheen().set_bit()); // フラッシュキャッシュ許可
}

// クロック設定
// 高速オンチップオシレータ(HOCO)を48MHzでメインクロックに設定する
#[allow(dead_code)]
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
    unsafe {
        core::ptr::write_volatile(0x4001_e037 as *mut u32, 0b0010_0000);
    }
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

#[derive(Debug, Clone, Copy)]
pub struct Rgb<T> {
    pub r: T,
    pub g: T,
    pub b: T,
}

const RAINBOW_TABLE: [Rgb<u8>; 7] = {
    let red = Rgb { r: 128, g: 0, b: 0 };
    let orange = Rgb {
        r: 128,
        g: 82,
        b: 0,
    };
    let yellow = Rgb {
        r: 128,
        g: 128,
        b: 0,
    };
    let green = Rgb { r: 0, g: 128, b: 0 };
    let cyan = Rgb {
        r: 0,
        g: 128,
        b: 128,
    };
    let blue = Rgb { r: 0, g: 0, b: 128 };
    let purple = Rgb {
        r: 128,
        g: 0,
        b: 128,
    };
    [red, orange, yellow, green, cyan, blue, purple]
};

fn ws2812b_reset(p: &pac::Peripherals, delay: &mut Delay, led_pin_bit: u16) {
    // OUTPUT LOW LEVEL
    p.PORT1
        .podr()
        .modify(|r, w| unsafe { w.bits(r.bits() & !led_pin_bit) });
    delay.delay_us(280);
}

fn ws2812b_write(p: &pac::Peripherals, led_pin_bit: u16, value: Rgb<u8>) {
    let grb = (value.g as u32) << 16 | (value.r as u32) << 8 | value.b as u32;
    for bit_digit in (0..=23u8).rev() {
        let flag = grb >> bit_digit & 1;
        // OUTPUT HIGH LEVEL
        p.PORT1
            .podr()
            .modify(|r, w| unsafe { w.bits(r.bits() | led_pin_bit) });
        if flag == 0 {
            cortex_m::asm::nop();
        } else {
            cortex_m::asm::nop();
            cortex_m::asm::nop();
            cortex_m::asm::nop();
        }
        // OUTPUT LOW LEVEL
        p.PORT1
            .podr()
            .modify(|r, w| unsafe { w.bits(r.bits() & !led_pin_bit) });
        cortex_m::asm::nop();
        cortex_m::asm::nop();
        cortex_m::asm::nop();
    }
}

// GPT320タイマオーバーフロー検出フラグ
static GPT320_TIMER_OVERFLOW_FLAG: AtomicBool = AtomicBool::new(false);

// タイマオーバーフロー割り込み番号
const GPT320_OVERFLOW_IEL: pac::Interrupt = pac::Interrupt::IEL10;

// タイマオーバーフロー割り込みハンドラ
#[cortex_m_rt::interrupt]
fn IEL10() {
    cortex_m::interrupt::free(|_cs| {
        let p = unsafe { pac::Peripherals::steal() };
        // タイマオーバーフロー割り込み
        if p.GPT320.gtst().read().tcfpo().is_1() {
            //
            GPT320_TIMER_OVERFLOW_FLAG.store(true, Ordering::SeqCst);
            // タイマオーバーフロー割り込みフラグクリア
            p.GPT320.gtst().modify(|_r, w| w.tcfpo().clear_bit());
            // 割り込みステータスフラグクリア
            p.ICU
                .ielsr(GPT320_OVERFLOW_IEL.number() as usize)
                .modify(|_r, w| w.ir().clear_bit());
        }
    })
}

#[cortex_m_rt::entry]
fn main() -> ! {
    // 周辺機能
    let p = pac::Peripherals::take().unwrap();
    let core = cortex_m::Peripherals::take().unwrap();

    // 48MHzクロック設定
    clock_init_hoco48(&p);
    let mut delay = Delay::new(core.SYST, 48_000_000);

    // PORT 106 = D6(WS2812B)
    // PORT 111 = D13(LED)
    // 以上の入出力ポートを出力に設定
    let led_pin_bit: u16 = 1 << 6 | 1 << 11;
    p.PORT1
        .pdr()
        .modify(|r, w| unsafe { w.bits(r.bits() | led_pin_bit) });

    //
    // 32ビットGPTタイマーの設定
    //

    // 32ビットGPTタイマーにクロックを供給する
    p.MSTP.mstpcrd().modify(
        |_r, w| w.mstpd5()._0(), // GPT323~320のモジュールストップ解除
    );

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

    // 割り込み有効
    unsafe { cortex_m::peripheral::NVIC::unmask(GPT320_OVERFLOW_IEL) };

    // GPT320タイマーカウント動作を開始
    p.GPT320.gtcr().modify(|_r, w| {
        w.cst()._1();
        w.md()._000(); // のこぎり波形PWMモード
        w.tpcs()._000() // プリスケーラ― (PCLKD/1)
    });

    // WS2812B消灯
    ws2812b_reset(&p, &mut delay, led_pin_bit);

    // メインループ
    let mut counter = 0;
    loop {
        if GPT320_TIMER_OVERFLOW_FLAG.swap(false, Ordering::SeqCst) {
            ws2812b_write(&p, led_pin_bit, RAINBOW_TABLE[counter]);
            counter = (counter + 1) % RAINBOW_TABLE.len();
        }
    }
}
