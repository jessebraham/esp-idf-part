//! Follows the specification outlined by ESP-IDF v5.3.1, which at the time of
//! writing is marked as the latest stable release.
//!
//! A single ESP32's flash can contain multiple apps, as well as many different
//! kinds of data (calibration data, filesystems, parameter storage, etc.). For
//! this reason a partition table is flashed to (default offset) 0x8000 in the
//! flash.
//!
//! For more information, see the ESP-IDF documentation:
//! <https://docs.espressif.com/projects/esp-idf/en/v5.3.1/esp32/api-guides/partition-tables.html>
//!
//! ## Examples
//!
//! ```rust
//! // TODO: Add an example :)
//! ```

#![doc(html_logo_url = "https://avatars.githubusercontent.com/u/46717278")]
#![deny(missing_debug_implementations, missing_docs, rust_2018_idioms)]
#![no_std]

use core::{
    cmp::{max, min},
    ops::Rem,
    str::FromStr,
};

use heapless::Vec;
use md5::{Digest, Md5};

const PARTITION_MAGIC_BYTES: [u8; 2] = [0xAA, 0x50];
const MAX_NAME_LENGTH: usize = 16; // Includes null terminator

const MAX_ENTRIES: usize = 95;
const PARTITION_SIZE: usize = 0x20; // 32B
const PARTITION_TABLE_SIZE: usize = 0xC00; // 3kB

const MD5_NUM_MAGIC_BYTES: usize = 16;
const MD5_PART_MAGIC_BYTES: [u8; MD5_NUM_MAGIC_BYTES] = [
    0xEB, 0xEB, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
];

type NameString = heapless::String<MAX_NAME_LENGTH>;

/// Errors encountered during creation or validation of a [PartitionTable]
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Error {
    /// Number of provided partitions exceeds the maximum allowable value
    TooManyPartitions(usize),
}

impl core::error::Error for Error {}

impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        todo!()
    }
}

/// Supported partition types
///
/// Partiton types can be specified as `app` (0x00) or `data` (0x01). Or, it can
/// be a number from 0-254 (or as hex, 0x00-0xFE). Types 0x00-0x3F are reserved
/// for ESP-IDF core functions.
///
/// If your app needs to store data in a format not already supported by
/// ESP-IDF, then please add a custom partition type value in the range
/// 0x40-0xFE.
///
/// The ESP-IDF bootloader ignores any partition types other than `app` (0x00)
/// and `data` (0x01).
///
/// For more information, see the ESP-IDF documentation:
/// <https://docs.espressif.com/projects/esp-idf/en/v5.3.1/esp32/api-guides/partition-tables.html#type-field>
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Type {
    /// App partition type
    App,
    /// Data partition type
    Data,
    /// Custom partition type
    Custom(u8),
}

impl Type {
    /// Value of the variant as a [u8]
    pub fn as_u8(&self) -> u8 {
        match self {
            Type::App => 0x00,
            Type::Data => 0x01,
            Type::Custom(value) => *value,
        }
    }
}

/// Supported partition subtypes
///
/// The 8-bit subtype field is specific to a given partition type. ESP-IDF
/// currently only specifies the meaning of the subtype field for `app` and
/// `data` partition types.
///
/// For more information, see the ESP-IDF documentation:
/// <https://docs.espressif.com/projects/esp-idf/en/v5.3.1/esp32/api-guides/partition-tables.html#subtype>
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum SubType {
    /// App partition subtype
    App(AppType),
    /// Data partition subtype
    Data(DataType),
}

impl SubType {
    /// Value of the variant as a [u8]
    pub fn as_u8(&self) -> u8 {
        match self {
            SubType::App(app_type) => app_type.as_u8(),
            SubType::Data(data_type) => data_type.as_u8(),
        }
    }
}

impl From<AppType> for SubType {
    fn from(value: AppType) -> Self {
        Self::App(value)
    }
}

impl From<DataType> for SubType {
    fn from(value: DataType) -> Self {
        Self::Data(value)
    }
}

/// Supported app partition subtypes
///
/// `factory` (0x00) is the default app partition. The bootloader will execute
/// the factory app unless there it sees a partition of type data/ota, in which
/// case it reads this partition to determine which OTA image to boot.
///
/// - OTA never updates the factory partition.
/// - If you want to conserve flash usage in an OTA project, you can remove the
///   factory partition and use `ota_0` instead.
///
/// `ota_0` (0x10) ... `ota_15` (0x1F) are the OTA app slots. When [OTA] is in
/// use, the OTA data partition configures which app slot the bootloader should
/// boot. When using OTA, an application should have at least two OTA
/// application slots (`ota_0` & `ota_1`). Refer to the [OTA documentation] for
/// more details.
///
/// `test` (0x20) is a reserved subtype for factory test procedures. It will be
/// used as the fallback boot partition if no other valid app partition is
/// found. It is also possible to configure the bootloader to read a GPIO input
/// during each boot, and boot this partition if the GPIO is held low, see [Boot
/// from Test Firmware].
///
/// For more information, see the ESP-IDF documentation:
/// <https://docs.espressif.com/projects/esp-idf/en/v5.3.1/esp32/api-guides/partition-tables.html#subtype>
///
/// [OTA]: https://docs.espressif.com/projects/esp-idf/en/v5.3.1/esp32/api-reference/system/ota.html
/// [OTA documentation]: https://docs.espressif.com/projects/esp-idf/en/v5.3.1/esp32/api-reference/system/ota.html
/// [Boot from Test Firmware]: https://docs.espressif.com/projects/esp-idf/en/v5.3.1/esp32/api-guides/bootloader.html#bootloader-boot-from-test-firmware
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum AppType {
    /// Factory app partition type
    Factory = 0x00,
    /// `ota_0` app partition type
    Ota0    = 0x10,
    /// `ota_1` app partition type
    Ota1    = 0x11,
    /// `ota_2` app partition type
    Ota2    = 0x12,
    /// `ota_3` app partition type
    Ota3    = 0x13,
    /// `ota_4` app partition type
    Ota4    = 0x14,
    /// `ota_5` app partition type
    Ota5    = 0x15,
    /// `ota_6` app partition type
    Ota6    = 0x16,
    /// `ota_7` app partition type
    Ota7    = 0x17,
    /// `ota_8` app partition type
    Ota8    = 0x18,
    /// `ota_9` app partition type
    Ota9    = 0x19,
    /// `ota_10` app partition type
    Ota10   = 0x1A,
    /// `ota_11` app partition type
    Ota11   = 0x1B,
    /// `ota_12` app partition type
    Ota12   = 0x1C,
    /// `ota_13` app partition type
    Ota13   = 0x1D,
    /// `ota_14` app partition type
    Ota14   = 0x1E,
    /// `ota_15` app partition type
    Ota15   = 0x1F,
    /// Test app partition type
    Test    = 0x20,
}

impl AppType {
    /// Value of the variant as a [u8]
    pub fn as_u8(&self) -> u8 {
        *self as u8
    }
}

/// Supported data partition subtypes
///
/// When type is `data`, the subtype field can be specified as `ota` (0x00),
/// `phy` (0x01), `nvs` (0x02), `nvs_keys` (0x04), or a range of other
/// component-specific subtypes.
///
/// `ota` (0x00) is the [OTA data partition] which stores information about the
/// currently selected OTA app slot. This partition should be 0x2000 bytes in
/// size. Refer to the [OTA documentation] for more details.
///
/// `phy` (0x01) is for storing PHY initialisation data. This allows PHY to be
/// configured per-device, instead of in firmware.
///
/// - In the default configuration, the phy partition is not used and PHY
///   initialisation data is compiled into the app itself. As such, this
///   partition can be removed from the partition table to save space.
/// - To load PHY data from this partition, open the project configuration menu
///   (`idf.py menuconfig`) and enable `CONFIG_ESP_PHY_INIT_DATA_IN_PARTITION`
///   option. You will also need to flash your devices with phy init data as the
///   esp-idf build system does not do this automatically.
///
/// `nvs` (0x02) is for the [Non-Volatile Storage (NVS) API].
///
/// - NVS is used to store per-device PHY calibration data (different to
///   initialisation data).
/// - NVS is used to store Wi-Fi data if the `esp_wifi_set_storage`
///   initialization function is used.
/// - The NVS API can also be used for other application data.
/// - It is strongly recommended that you include an NVS partition of at least
///   0x3000 bytes in your project.
/// - If using NVS API to store a lot of data, increase the NVS partition size
///   from the default 0x6000 bytes.
///
/// `nvs_keys` (0x04) is for the NVS key partition. See [Non-Volatile Storage
/// (NVS) API] for more details.
///
/// - It is used to store NVS encryption keys when NVS Encryption feature is
///   enabled.
/// - The size of this partition should be 4096 bytes (minimum partition size).
///
/// There are other predefined data subtypes for data storage supported by
/// ESP-IDF. These include:
///
/// - `coredump` (0x03) is for storing core dumps while using a custom partition
///   table CSV file. See [Core Dump] for more details.
/// - `efuse` (0x05) is for emulating eFuse bits using [Virtual eFuses].
/// - `undefined` (0x06) is implicitly used for data partitions with unspecified
///   (empty) subtype, but it is possible to explicitly mark them as undefined
///   as well.
/// - `fat` (0x81) is for [FAT Filesystem Support].
/// - `spiffs` (0x82) is for [SPIFFS Filesystem].
/// - `littlefs` (0x83) is for [LittleFS filesystem].
///
/// If the partition type is any application-defined value (range 0x40-0xFE),
/// then `subtype` field can be any value chosen by the application (range
/// 0x00-0xFE).
///
/// For more information, see the ESP-IDF documentation:
/// <https://docs.espressif.com/projects/esp-idf/en/v5.3.1/esp32/api-guides/partition-tables.html#subtype>
///
/// [OTA data partition]: https://docs.espressif.com/projects/esp-idf/en/v5.3.1/esp32/api-reference/system/ota.html#ota-data-partition
/// [OTA documentation]: https://docs.espressif.com/projects/esp-idf/en/v5.3.1/esp32/api-reference/system/ota.html#ota-data-partition
/// [Non-Volatile Storage (NVS) API]: https://docs.espressif.com/projects/esp-idf/en/v5.3.1/esp32/api-reference/storage/nvs_flash.html
/// [Core Dump]: https://docs.espressif.com/projects/esp-idf/en/v5.3.1/esp32/api-guides/core_dump.html
/// [Virtual eFuses]: https://docs.espressif.com/projects/esp-idf/en/v5.3.1/esp32/api-reference/system/efuse.html#virtual-efuses
/// [Fat Filesystem Support]: https://docs.espressif.com/projects/esp-idf/en/v5.3.1/esp32/api-reference/storage/fatfs.html
/// [SPIFFS Filesystem]: https://docs.espressif.com/projects/esp-idf/en/v5.3.1/esp32/api-reference/storage/spiffs.html
/// [LittleFS filesystem]: https://github.com/littlefs-project/littlefs
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum DataType {
    /// OTA data partition which stores information about the currently selected
    /// OTA app slot
    Ota       = 0x00,
    /// PHY initialisation data
    Phy       = 0x01,
    /// Used for the [Non-Volatile Storage (NVS) API]
    ///
    /// [Non-Volatile Storage (NVS) API]: https://docs.espressif.com/projects/esp-idf/en/v5.3.1/esp32/api-reference/storage/nvs_flash.html
    Nvs       = 0x02,
    /// For storing core dumps while using a custom partition table CSV file
    Coredump  = 0x03,
    /// NVS key partition
    ///
    /// See [Non-Volatile Storage (NVS) API] for more details.
    ///
    /// [Non-Volatile Storage (NVS) API]: https://docs.espressif.com/projects/esp-idf/en/v5.3.1/esp32/api-reference/storage/nvs_flash.html
    NvsKeys   = 0x04,
    /// Used for emulating eFuse bits using Virtual eFuses
    EfuseEm   = 0x05,
    /// Implicitly used for data partitions with unspecified (empty) subtype
    Undefined = 0x06,
    /// TODO: Document me!
    EspHttpd  = 0x80,
    /// FAT filesystem support
    Fat       = 0x81,
    /// SPIFFS filesystem support
    Spiffs    = 0x82,
    /// LittleFS filesystem support
    LittleFs  = 0x83,
}

impl DataType {
    /// Value of the variant as a [u8]
    pub fn as_u8(&self) -> u8 {
        *self as u8
    }
}

bitflags::bitflags! {
    /// Supported partition flags
    ///
    /// Two flags are currently supported, `encrypted` and `readonly`:
    ///
    /// - If `encrypted` flag is set, the partition will be encrypted if [Flash
    ///   Encryption] is enabled.
    ///     - Note: `app` type partitions will always be encrypted, regardless of
    ///       whether this flag is set or not.
    /// - If `readonly` flag is set, the partition will be read-only. This flag is
    ///   only supported for `data` type partitions except `ota` and `coredump`
    ///   subtypes. This flag can help to protect against accidental writes to a
    ///   partition that contains critical device-specific configuration data, e.g.
    ///   factory data partition.
    ///
    /// You can specify multiple flags by separating them with a colon. For example,
    /// `encrypted:readonly`.
    ///
    /// For more information, see the ESP-IDF documentation:
    /// <https://docs.espressif.com/projects/esp-idf/en/v5.3.1/esp32/api-guides/partition-tables.html#flags>
    ///
    /// [Flash Encryption]: https://docs.espressif.com/projects/esp-idf/en/v5.3.1/esp32/security/flash-encryption.html
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct Flags: u32 {
        /// Encrypted partition
        const ENCRYPTED = 0b0001;
        /// Read-only partition
        const READONLY  = 0b0010;
    }
}

/// A partition
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[repr(C)]
pub struct Partition {
    magic: [u8; 2],
    type_: Type,
    subtype: SubType,
    offset: u32,
    size: u32,
    name: NameString,
    flags: Flags,
}

impl Partition {
    /// Construct a new instance of [Partition]
    pub fn new(
        name: &str,
        type_: Type,
        subtype: SubType,
        offset: u32,
        size: u32,
        flags: Flags,
    ) -> Self {
        // The name of the partition can be at most `MAX_NAME_LENGTH` bytes in length,
        // *including* the null terminator. Names longer than this are truncated.
        let length = min(name.len(), MAX_NAME_LENGTH - 1);
        let name = NameString::from_str(&name[..length]).unwrap();

        Self {
            magic: PARTITION_MAGIC_BYTES,
            type_,
            subtype,
            offset,
            size,
            name,
            flags,
        }
    }

    /// Name of the partition
    pub fn name(&self) -> &str {
        self.name.as_str()
    }

    /// [Type] of the partition
    pub fn type_(&self) -> Type {
        self.type_
    }

    /// [SubType] of the partition
    pub fn subtype(&self) -> SubType {
        self.subtype
    }

    /// Offest of the partition
    pub fn offset(&self) -> u32 {
        self.offset
    }

    /// Size of the partition
    pub fn size(&self) -> u32 {
        self.size
    }

    /// Flags of the partition
    pub fn flags(&self) -> Flags {
        self.flags
    }

    /// Convert a partition into an array of bytes
    pub fn as_bytes(&self) -> [u8; PARTITION_SIZE] {
        let mut name = [b'\0'; MAX_NAME_LENGTH];
        for (i, byte) in self.name.as_bytes().iter().enumerate() {
            name[i] = *byte;
        }

        let mut bytes = Vec::<u8, PARTITION_SIZE>::new();
        bytes.extend(self.magic);
        bytes.extend([self.type_.as_u8()]);
        bytes.extend([self.subtype.as_u8()]);
        bytes.extend(self.offset.to_le_bytes());
        bytes.extend(self.size.to_le_bytes());
        bytes.extend(name);
        bytes.extend(self.flags.bits().to_le_bytes());

        // SAFETY: We know that `bytes` is the correct length, so this will not panic
        bytes.into_array().unwrap()
    }
}

impl PartialOrd for Partition {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        self.offset.partial_cmp(&other.offset)
    }
}

/// A partition table
#[derive(Debug, Default, Clone, PartialEq, Eq, Hash)]
#[repr(C)]
pub struct PartitionTable {
    partitions: Vec<Partition, MAX_ENTRIES>,
}

impl PartitionTable {
    /// Construct a new instance of [PartitionTable]
    pub fn new(partitions: &[Partition]) -> Result<Self, Error> {
        let partitions =
            Vec::from_slice(partitions).map_err(|_| Error::TooManyPartitions(partitions.len()))?;

        Ok(Self { partitions })
    }

    /// Returns a slice of [Partition]
    pub fn partitions(&self) -> &[Partition] {
        &self.partitions
    }

    /// Find a partition with the given name
    ///
    /// This function is short-circuiting; it will return the first partition
    /// found, if one exists.
    pub fn find(&self, name: &str) -> Option<&Partition> {
        self.partitions.iter().find(|p| p.name() == name)
    }

    /// Find a partition with the given [Type]
    ///
    /// This function is short-circuiting; it will return the first partition
    /// found, if one exists.
    pub fn find_type(&self, type_: Type) -> Option<&Partition> {
        self.partitions.iter().find(|p| p.type_() == type_)
    }

    /// Find a partition with the given [SubType]
    ///
    /// This function is short-circuiting; it will return the first partition
    /// found, if one exists.
    pub fn find_subtype(&self, subtype: SubType) -> Option<&Partition> {
        self.partitions.iter().find(|p| p.subtype() == subtype)
    }

    /// Validate a partition table
    pub fn validate(&self) -> Result<(), Error> {
        todo!()
    }

    /// Convert a partition table into an array of bytes
    pub fn as_bytes(&self) -> [u8; PARTITION_TABLE_SIZE] {
        let mut bytes = Vec::<u8, PARTITION_TABLE_SIZE>::new();
        let mut hasher = Md5::new();

        for partition in &self.partitions {
            let partition_bytes = partition.as_bytes();
            bytes.extend(partition_bytes);
            hasher.update(partition_bytes);
        }

        bytes.extend(MD5_PART_MAGIC_BYTES);
        bytes.extend(hasher.finalize());

        let padding = core::iter::repeat(0xFF).take(PARTITION_TABLE_SIZE - bytes.len());
        bytes.extend(padding);

        // SAFETY: We know that `bytes` is the correct length, so this will not panic
        bytes.into_array().unwrap()
    }
}
