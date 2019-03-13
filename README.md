# fw_env - simply read a variable from U-Boot Env block

## What it is

`fw_env` is a simple pure Rust implementation of `fw_printenv` from the [U-Boot](https://www.denx.de/wiki/U-Boot) bootloader tools.
Basic usage:
```rust
	let config = Config::init()?;
	let env = FwEnv::read(&config)?;
	let uboot_version = env.find_var(b"ver"[..]).expect("variable not found");
```

## TODO

- [X] check CRC
- [X] find variable by name
- [ ] full config file (`/etc/fw_env.config`) parsing
- [ ] handle bad blocks in flash
- [ ] handle flags in redundant blocks
