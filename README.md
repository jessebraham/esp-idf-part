# esp-idf-part

![GitHub Workflow Status](https://img.shields.io/github/actions/workflow/status/esp-rs/esp-idf-part/ci.yml?label=CI&logo=github&style=flat-square)
[![Crates.io](https://img.shields.io/crates/v/esp-idf-part?color=C96329&logo=Rust&style=flat-square)](https://crates.io/crates/esp-idf-part)
[![docs.rs](https://img.shields.io/docsrs/esp-idf-part?color=C96329&logo=rust&style=flat-square)](https://docs.rs/esp-idf-part)
![MSRV](https://img.shields.io/badge/MSRV-1.61-blue?style=flat-square)
![Crates.io](https://img.shields.io/crates/l/esp-idf-part?style=flat-square)

A library for parsing and generating ESP-IDF partition tables. Supports parsing from and generating to both CSV and binary formats.

This package started its life as a module in [espflash](https://github.com/esp-rs/espflash/), however it has undergone some fairly extensive changes since being extracted into its own crate. A big thanks to all who contributed to that module, as their work helped make this library possible.

This library is reasonably well tested, however if you have a partition table which is not handled correctly by this library then please open an issue.

## Resources

- [ESP-IDF Partition Table API Guide](https://docs.espressif.com/projects/esp-idf/en/latest/esp32/api-guides/partition-tables.html)

## Features

This library has only one feature, `std`, which is enabled by default. While this library _does_ support `no_std`, (de)serialization functionality is not supported without `std`.

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in
the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without
any additional terms or conditions.
