# swift-v5

> Build Embedded Swift programs for VEX!

swift-v5 is a command line tool that simplifies the development process for Embedded Swift programs targeting the VEX V5 platform.

## Installation

### Install via shell script

Works best for Unix systems: macOS, Linux

```sh
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/vexide/swift-v5/releases/latest/download/swift-v5-installer.sh | sh
```

### Install via Homebrew

Works best for people who have Homebrew installed

```sh
brew install vexide/vexide/swift-v5
```

### Install via powershell script

Works best for Windows

```powershell
irm https://github.com/vexide/swift-v5/releases/latest/download/swift-v5-installer.ps1 | iex
```

### Install via cargo binstall

Works on all platforms but you need cargo-binstall installed

```sh
cargo binstall swift-v5 --git "https://github.com/vexide/swift-v5"
```

### Build from source

Works on all platforms but it takes a long time

```sh
cargo install --git "https://github.com/vexide/swift-v5"
```

## Usage

swift-v5 can manage the Arm Toolchain for Embedded version your Swift project uses.
Run `swift v5 install` to download the latest version of the toolchain.

You can also make place a config file named `v5.toml` next to your `Package.swift`
to specify which version of the toolchain swift-v5 should download:

```toml
# v5.toml

llvm-version = "20.1.0"
```
