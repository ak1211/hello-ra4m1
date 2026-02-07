Arduino UNO R4 MINIMA でLEDを点滅させる

## ビルドと書き込み方法
Arduino UNO R4 MINIMA の SWDコネクターとDAPLINKを接続して `cargo run` する

hello-ra4m1\panic_probe> cargo run
   Compiling hello-ra4m1 v0.1.0 (C:\Users\aki\development\hello-ra4m1\panic_probe)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.47s
     Running `probe-rs run --chip R7FA4M1AB target\thumbv7em-none-eabihf\debug\hello-ra4m1`
      Erasing ✔ 100% [####################]  36.00 KiB @  15.46 KiB/s (took 2s)
  Programming ✔ 100% [####################]  36.00 KiB @   5.62 KiB/s (took 6s)                                             
     Finished in 8.83s
[INFO ] Hello. I'm "R7FA4M1AB3CFM   " (hello_ra4m1 panic_probe/src/main.rs:118)
