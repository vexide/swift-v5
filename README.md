# swift-v5

> Build Embedded Swift programs for VEX!

swift-v5 is a command line tool that simplifies the development process for Embedded Swift programs targeting the VEX V5 platform.

## Installation

Currently, swift-v5 must be built from source using Cargo:

```
cargo install swift-v5
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