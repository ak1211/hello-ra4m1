// hello-ra4m1
// RA4M1-Zero Mini Development Boardに乗っているWS2812BでLチカする
//
// SPDX-License-Identifier: MIT
// SPDX-FileCopyrightText: 2025 Akihiro Yamamoto <github.com/ak1211>

#![no_std]
#![no_main]

use panic_halt as _;
use ra4m1_fsp_pac as pac;
use scopeguard::defer;

// クロック設定
// 16MHz水晶発振子をメインクロックに設定する
#[allow(dead_code)]
fn clock_init_xtal(system: &pac::SYSTEM, p: &pac::Peripherals) {
    // 保護レジスタを操作して書込み許可を与える
    system.prcr().write(|w| {
        w.prkey().set(0xa5);
        w.prc0().set_bit(); // クロック発生回路関連レジスタに書込み許可を与える
        w.prc1().set_bit() // 低消費電力モード関連レジスタに書込み許可を与える
    });
    // 関数脱出時に保護レジスタを元通りに復帰する
    defer! {
        system.prcr().write(|w| {
            w.prkey().set(0xa5);
            w.prc0().clear_bit();
            w.prc1().clear_bit()
        });
    }

    // 消費電力モードはハイスピードモードに設定
    system.opccr().write(|w| w.opcm()._00());
    while !system.opccr().read().opcmtsf().bit_is_clear() {} // 確認

    // サブクロックの停止
    system.sosccr().write(|w| w.sostp().set_bit()); // サブクロックの停止
    while !system.sosccr().read().sostp().bit_is_set() {} // サブクロック停止確認

    // メインクロック発振器(MOSC)の停止
    system.mosccr().write(|w| w.mostp()._1());
    while !system.mosccr().read().mostp().is_1() {} // 確認

    // メインクロック発振器(MOSC)モードコントロールレジスタ
    system.momcr().write(|w| {
        w.modrv1()._0(); // 10MHz ～ 20MHz
        w.mosel()._0() // 外部水晶発振子
    });

    // メインクロック発振器(MOSC)待機時間
    system.moscwtcr().write(|w| w.msts()._1001()); // 32768us

    // メインクロック発振器(MOSC)動作
    system.mosccr().write(|w| w.mostp()._0());
    while !system.mosccr().read().mostp().is_0() {} // 確認

    // メインクロック発振器(MOSC)発振安定待ち
    while !system.oscsf().read().moscsf().bit_is_set() {}

    // 分周器設定
    system.sckdivcr().write(|w| {
        w.ick()._000(); // システムクロック(ICLK Div /1)
        w.pcka()._000(); // 周辺モジュールクロックA(PCLKA Div /1)
        w.pckb()._000(); // 周辺モジュールクロックB(PCLKA Div /1)
        w.pckc()._000(); // 周辺モジュールクロックC(PCLKA Div /1)
        w.pckd()._000(); // 周辺モジュールクロックD(PCLKA Div /1)
        w.fck()._000() // Flashインターフェースクロック(FCLK Div /1)
    });

    // システムクロックをメインクロックに切り替え
    system.sckscr().write(|w| w.cksel()._011()); // メインクロック発振器(MOSC)
    while !system.sckscr().read().cksel().is_011() {} // 確認

    // フラッシュキャッシュ
    p.FCACHE.fcacheiv().write(|w| w.fcacheiv()._1()); // フラッシュキャッシュインバリデート

    while p.FCACHE.fcacheiv().read().fcacheiv().bit_is_set() {}
    p.FCACHE.fcachee().write(|w| w.fcacheen().set_bit()); // フラッシュキャッシュ許可
}

// クロック設定
// 16MHz水晶発振子を12逓倍のち4分周した48MHzをクロックに設定する
#[allow(dead_code)]
fn clock_init_pll48(system: &pac::SYSTEM, p: &pac::Peripherals) {
    // 保護レジスタを操作して書込み許可を与える
    system.prcr().write(|w| {
        w.prkey().set(0xa5);
        w.prc0().set_bit(); // クロック発生回路関連レジスタに書込み許可を与える
        w.prc1().set_bit() // 低消費電力モード関連レジスタに書込み許可を与える
    });
    // 関数脱出時に保護レジスタを元通りに復帰する
    defer! {
        system.prcr().write(|w| {
            w.prkey().set(0xa5);
            w.prc0().clear_bit();
            w.prc1().clear_bit()
        });
    }

    // 消費電力モードはハイスピードモードに設定
    system.opccr().write(|w| w.opcm()._00());
    while !system.opccr().read().opcmtsf().bit_is_clear() {} // 確認

    // サブクロックの停止
    system.sosccr().write(|w| w.sostp().set_bit()); // サブクロックの停止
    while !system.sosccr().read().sostp().bit_is_set() {} // サブクロック停止確認

    // メインクロック発振器(MOSC)の停止
    system.mosccr().write(|w| w.mostp()._1());
    while !system.mosccr().read().mostp().is_1() {} // 確認

    //
    // メインクロック発振器(MOSC)の入力は16MHz水晶発振子
    //

    // メインクロック発振器(MOSC)モードコントロールレジスタ
    system.momcr().write(|w| {
        w.modrv1()._0(); // 10MHz ～ 20MHz
        w.mosel()._0() // 外部水晶発振子
    });

    // メインクロック発振器(MOSC)待機時間
    system.moscwtcr().write(|w| w.msts()._1001()); // 32768us

    // メインクロック発振器(MOSC)動作
    system.mosccr().write(|w| w.mostp()._0());
    while !system.mosccr().read().mostp().is_0() {} // 確認

    // メインクロック発振器(MOSC)発振安定待ち
    while !system.oscsf().read().moscsf().bit_is_set() {}

    // メインクロック発振器(MOSC)をPLLで逓倍する
    // 逓倍率および分周比の設定
    system.pllccr2().write(|w| {
        w.pllmul().set(12 - 1); // PLL Mul x12
        w.plodiv()._10() // PLL Div /4
    });

    // PLL動作
    system.pllcr().write(|w| w.pllstp()._0());
    while !system.pllcr().read().pllstp().is_0() {} // 確認

    // PLL発振安定待ち
    while !system.oscsf().read().pllsf().bit_is_set() {}

    // 分周器設定
    system.sckdivcr().write(|w| {
        w.ick()._000(); // システムクロック(ICLK Div /1)
        w.pcka()._000(); // 周辺モジュールクロックA(PCLKA Div /1)
        w.pckb()._001(); // 周辺モジュールクロックB(PCLKA Div /2)
        w.pckc()._000(); // 周辺モジュールクロックC(PCLKA Div /1)
        w.pckd()._000(); // 周辺モジュールクロックD(PCLKA Div /1)
        w.fck()._001() // Flashインターフェースクロック(FCLK Div /2)
    });

    // システムクロックをPLLに切り替え
    system.sckscr().write(|w| w.cksel()._101()); // PLL
    while !system.sckscr().read().cksel().is_101() {} // 確認

    // フラッシュキャッシュ
    p.FCACHE.fcacheiv().write(|w| w.fcacheiv()._1()); // フラッシュキャッシュインバリデート

    while p.FCACHE.fcacheiv().read().fcacheiv().bit_is_set() {}
    p.FCACHE.fcachee().write(|w| w.fcacheen().set_bit()); // フラッシュキャッシュ許可
}

// クロック設定
// 高速オンチップオシレータ(HOCO)をメインクロックに設定する
#[allow(dead_code)]
fn clock_init_hoco(system: &pac::SYSTEM, p: &pac::Peripherals) {
    // 保護レジスタを操作して書込み許可を与える
    system.prcr().write(|w| {
        w.prkey().set(0xa5);
        w.prc0().set_bit(); // クロック発生回路関連レジスタに書込み許可を与える
        w.prc1().set_bit() // 低消費電力モード関連レジスタに書込み許可を与える
    });
    // 関数脱出時に保護レジスタを元通りに復帰する
    defer! {
        system.prcr().write(|w| {
            w.prkey().set(0xa5);
            w.prc0().clear_bit();
            w.prc1().clear_bit()
        });
    }

    // 消費電力モードはハイスピードモードに設定
    system.opccr().write(|w| w.opcm()._00());
    while !system.opccr().read().opcmtsf().bit_is_clear() {} // 確認

    // サブクロックの停止
    system.sosccr().write(|w| w.sostp().set_bit()); // サブクロックの停止
    while !system.sosccr().read().sostp().bit_is_set() {} // サブクロック停止確認

    // 高速オンチップオシレータ(HOCO)クロック動作
    system.hococr().write(|w| w.hcstp()._0());
    while !system.hococr().read().hcstp().is_0() {} // 確認

    // 高速オンチップオシレータ(HOCO)クロック発振安定待ち
    while !system.oscsf().read().hocosf().bit_is_set() {}

    // 分周器設定
    system.sckdivcr().write(|w| {
        w.ick()._000(); // システムクロック(ICLK Div /1)
        w.pcka()._000(); // 周辺モジュールクロックA(PCLKA Div /1)
        w.pckb()._000(); // 周辺モジュールクロックB(PCLKA Div /1)
        w.pckc()._000(); // 周辺モジュールクロックC(PCLKA Div /1)
        w.pckd()._000(); // 周辺モジュールクロックD(PCLKA Div /1)
        w.fck()._000() // Flashインターフェースクロック(FCLK Div /1)
    });

    // システムクロックを高速オンチップオシレータ(HOCO)クロックに切り替え
    system.sckscr().write(|w| w.cksel()._000()); // HOCOクロック
    while !system.sckscr().read().cksel().is_000() {} // 確認

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

fn ws2812b_reset(p: &pac::Peripherals, led_pin_bit: u16) {
    // OUTPUT LOW LEVEL
    p.PORT1
        .podr()
        .modify(|r, w| unsafe { w.bits(r.bits() & !led_pin_bit) });
    for _ in 0..1_000_000 {
        cortex_m::asm::nop();
    }
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

#[cortex_m_rt::entry]
fn main() -> ! {
    let system = unsafe { pac::SYSTEM::steal() };

    // 周辺機能
    let p = pac::Peripherals::take().unwrap();

    // クロック設定
    clock_init_pll48(&system, &p);

    // PORT 106 = D6(WS2812B)
    // PORT 111 = D13(LED)
    // 以上の入出力ポートを出力に設定
    let led_pin_bit: u16 = 1 << 6 | 1 << 11;
    p.PORT1
        .pdr()
        .modify(|r, w| unsafe { w.bits(r.bits() | led_pin_bit) });

    // 色
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

    // 色順
    let sequences = [red, orange, yellow, green, cyan, blue, purple];

    // メインループ
    loop {
        for color in sequences {
            ws2812b_reset(&p, led_pin_bit);
            ws2812b_write(&p, led_pin_bit, color);
            for _ in 0..1_000_000 {
                cortex_m::asm::nop();
            }
        }
    }
}
