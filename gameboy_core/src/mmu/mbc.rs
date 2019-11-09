use super::cartridge::Cartridge;

pub trait Mbc {
    fn read_byte(&self, index: u16) -> u8;
    fn write_byte(&mut self, index: u16, value: u8);
    fn get_cartridge(&self) -> &Cartridge;
}