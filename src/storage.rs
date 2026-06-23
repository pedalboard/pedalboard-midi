//! Flash-based persistent configuration storage for RP2040.
//!
//! Uses the last 64KB of flash (16 pages of 4KB each) with `sequential-storage`
//! for wear-leveled key-value storage. Flash operations use `rp2040-flash` which
//! executes from RAM to avoid XIP conflicts.

use embedded_storage::nor_flash::{NorFlashError, NorFlashErrorKind};
use embedded_storage_async::nor_flash::{ErrorType, MultiwriteNorFlash, NorFlash, ReadNorFlash};
use sequential_storage::cache::NoCache;
use sequential_storage::map::{MapConfig, MapStorage, SerializationError, Value};

const STORAGE_ORIGIN: u32 = 0x001F_0000; // offset from flash start (2MB - 64KB)
const STORAGE_SIZE: usize = 64 * 1024;
const SECTOR_SIZE: usize = 4096;
const PAGE_SIZE: usize = 256;

#[derive(Debug)]
pub struct FlashError;

impl NorFlashError for FlashError {
    fn kind(&self) -> NorFlashErrorKind {
        NorFlashErrorKind::Other
    }
}

pub struct FlashStorage;

impl ErrorType for FlashStorage {
    type Error = FlashError;
}

impl ReadNorFlash for FlashStorage {
    const READ_SIZE: usize = 1;

    async fn read(&mut self, offset: u32, bytes: &mut [u8]) -> Result<(), Self::Error> {
        let addr = (0x1000_0000 + STORAGE_ORIGIN + offset) as *const u8;
        for (i, byte) in bytes.iter_mut().enumerate() {
            *byte = unsafe { core::ptr::read_volatile(addr.add(i)) };
        }
        Ok(())
    }

    fn capacity(&self) -> usize {
        STORAGE_SIZE
    }
}

impl NorFlash for FlashStorage {
    const WRITE_SIZE: usize = 4;
    const ERASE_SIZE: usize = SECTOR_SIZE;

    async fn erase(&mut self, from: u32, to: u32) -> Result<(), Self::Error> {
        let flash_addr = STORAGE_ORIGIN + from;
        let len = to - from;
        cortex_m::interrupt::free(|_| unsafe {
            rp2040_flash::flash::flash_range_erase(flash_addr, len, true);
        });
        Ok(())
    }

    async fn write(&mut self, offset: u32, bytes: &[u8]) -> Result<(), Self::Error> {
        let flash_addr = STORAGE_ORIGIN + offset;
        // rp2040 ROM requires 256-byte aligned address and length multiple of 256
        let aligned_addr = flash_addr & !0xFF;
        let start_offset = (flash_addr - aligned_addr) as usize;
        // Write one 256-byte page with data positioned at the correct offset
        let mut page_buf = [0xFFu8; PAGE_SIZE];
        let copy_len = bytes.len().min(PAGE_SIZE - start_offset);
        page_buf[start_offset..start_offset + copy_len].copy_from_slice(&bytes[..copy_len]);
        cortex_m::interrupt::free(|_| unsafe {
            rp2040_flash::flash::flash_range_program(aligned_addr, &page_buf, true);
        });
        Ok(())
    }
}

impl MultiwriteNorFlash for FlashStorage {}

/// Config value stored in flash (u16).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConfigValue(pub u16);

impl<'a> Value<'a> for ConfigValue {
    fn serialize_into(&self, buffer: &mut [u8]) -> Result<usize, SerializationError> {
        if buffer.len() < 2 {
            return Err(SerializationError::BufferTooSmall);
        }
        buffer[0] = (self.0 >> 8) as u8;
        buffer[1] = self.0 as u8;
        Ok(2)
    }
    fn deserialize_from(buffer: &'a [u8]) -> Result<(Self, usize), SerializationError> {
        if buffer.len() < 2 {
            return Err(SerializationError::BufferTooSmall);
        }
        Ok((ConfigValue(((buffer[0] as u16) << 8) | buffer[1] as u16), 2))
    }
}

/// Encode a config key: block(3 bits) | section(5 bits) | index(8 bits) = u16
pub fn encode_key(block: u8, section: u8, index: u8) -> u16 {
    ((block as u16) << 13) | ((section as u16) << 8) | index as u16
}

/// Persistent config store wrapping sequential-storage map.
pub struct ConfigStore {
    map: MapStorage<u16, FlashStorage, NoCache>,
    buf: [u8; SECTOR_SIZE],
}

impl ConfigStore {
    pub fn try_new() -> Option<Self> {
        let config = MapConfig::try_new(0..STORAGE_SIZE as u32)?;
        Some(Self {
            map: MapStorage::new(FlashStorage, config, NoCache::new()),
            buf: [0u8; SECTOR_SIZE],
        })
    }

    /// Store a config value.
    pub async fn save(&mut self, block: u8, section: u8, index: u8, value: u16) {
        let key = encode_key(block, section, index);
        let _ = self
            .map
            .store_item(&mut self.buf, &key, &ConfigValue(value))
            .await;
    }

    /// Load a config value. Returns None if not found.
    pub async fn load(&mut self, block: u8, section: u8, index: u8) -> Option<u16> {
        let key = encode_key(block, section, index);
        self.map
            .fetch_item::<ConfigValue>(&mut self.buf, &key)
            .await
            .ok()
            .flatten()
            .map(|v| v.0)
    }

    /// Erase all stored config (factory reset).
    pub async fn erase_all(&mut self) {
        let _ = self.map.erase_all().await;
    }

    /// Load all stored config entries. Returns (block, section, index, value) tuples.
    pub async fn load_all(&mut self) -> heapless::Vec<(u8, u8, u8, u16), 128> {
        let mut entries = heapless::Vec::new();
        let Ok(mut iter) = self.map.fetch_all_items(&mut self.buf).await else {
            return entries;
        };
        let mut item_buf = [0u8; 64];
        while let Ok(Some((key, ConfigValue(value)))) =
            iter.next::<ConfigValue>(&mut item_buf).await
        {
            let block = ((key >> 13) & 0x07) as u8;
            let section = ((key >> 8) & 0x1F) as u8;
            let index = (key & 0xFF) as u8;
            entries.push((block, section, index, value)).ok();
        }
        entries
    }
}
