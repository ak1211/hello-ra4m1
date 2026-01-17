[Waveshare RA4M1-Zero Mini Development Board](https://www.waveshare.com/ra4m1-zero.htm) のsmart ledを色を変えながら点灯させる。

## ビルド方法
Windows 11のPowerShell 上で以下のコマンドでビルドする。

```
PS C:\hello-ra4m1\pac> cargo objcopy --release -- -O ihex app.hex
   Compiling hello-ra4m1 v0.1.0 (C:\hello-ra4m1\pac)
    Finished `release` profile [optimized] target(s) in 1.76s
PS C:\hello-ra4m1\pac> 
```
ビルドして出来た app.hex をRA4M1に書込む。

## 書き込み方法
RA4M1-Zero ボード上の BOOT と RESET ボタンを同時押しで "RA USB Boot" の状態にして Renesas Flash Programmerで書込む。

## 実行方法
RESETボタンを押して実行する。
