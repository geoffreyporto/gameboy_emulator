pub mod color;
pub mod lcd_control_flag;
pub mod lcd_status_flag;
mod sprite_attributes;

use self::color::Color;
use self::lcd_control_flag::LcdControlFlag;
use self::sprite_attributes::SpriteAttributes;
use emulator::traits::PixelMapper;
use mmu::interrupt::Interrupt;
use mmu::Memory;

const SPRITES_START_INDEX: u16 = 0xFE00;
const LCD_CONTROL_INDEX: u16 = 0xFF40;
const LCD_INDEX: u16 = 0xFF41;
const SCROLL_Y_INDEX: u16 = 0xFF42;
const SCROLL_X_INDEX: u16 = 0xFF43;
const _LYC_INDEX: u16 = 0xFF45;
const BACKGROUND_PALETTE_INDEX: u16 = 0xFF47;
const OBJECT_PALETTE_0_INDEX: u16 = 0xFF48;
const OBJECT_PALETTE_1_INDEX: u16 = 0xFF49;
const WINDOW_Y_INDEX: u16 = 0xFF4A;
const WINDOW_X_INDEX: u16 = 0xFF4B;

const HBLANK: u8 = 0b00;
const VBLANK: u8 = 0b01;
const OAM_SCAN: u8 = 0b10;
const LCD_TRANSFER: u8 = 0b11;

const GAMEBOY_WIDTH: i32 = 160;
const GAMEBOY_HEIGHT: i32 = 144;

pub struct GPU {
    tile_cycles_counter: i32,
    vblank_line: i32,
    scan_line_transfered: bool,
    hide_frames: i32
}

impl GPU {
    // TODO: make a background color map that we use to check bg priority
    pub fn new() -> GPU {
        GPU {
            tile_cycles_counter: 0,
            vblank_line: 0,
            hide_frames: 0,
            scan_line_transfered: false
        }
    }

    // return value indicated wheather a vblank has happened
    // true -> vblank has happened, render the framebuffer
    // false -> no vblank, continue stepping
    pub fn step<T: PixelMapper>(
        &mut self,
        cycles: i32,
        memory: &mut Memory,
        system: &mut T,
    ) -> bool {
        let mut vblank = false;
        memory.gpu_cycles.cycles_counter += cycles;

        if !memory.screen_disabled {
            match memory.lcd_status_mode {
                HBLANK => {
                    if memory.gpu_cycles.cycles_counter >= 204 {
                        memory.gpu_cycles.cycles_counter -= 204;
                        memory.lcd_status_mode = (memory.lcd_status_mode & 0b1111_1100) | OAM_SCAN;

                        memory.scan_line += 1;
                        memory.compare_ly_to_lyc();

                        if memory.scan_line == 144 {
                            memory.lcd_status_mode =
                                (memory.lcd_status_mode & 0b1111_1100) | VBLANK;
                            self.vblank_line = 0;
                            memory.gpu_cycles.aux_cycles_counter = memory.gpu_cycles.cycles_counter;

                            memory.request_interrupt(Interrupt::Vblank);    

                            memory.irq48_signal &= 0x09;
                            let stat = memory.get_lcd_status_from_memory();
                            if stat & 0b0001_0000 == 0b0001_0000 {
                                if memory.irq48_signal & 0b0000_0001 != 0b0000_0001
                                    && memory.irq48_signal & 0b0000_1000 != 0b0000_1000
                                {
                                    memory.request_interrupt(Interrupt::Lcd);
                                }
                                memory.irq48_signal |= 0b0000_0010;
                            }
                            memory.irq48_signal &= 0x0E;

                            if self.hide_frames > 0 {
                                self.hide_frames -= 1;
                            } else {
                                vblank = true;
                            }

                            memory.gpu_cycles.window_line = 0;
                        } else {
                            memory.irq48_signal &= 0x09;
                            let stat = memory.get_lcd_status_from_memory();

                            if stat & 0b0010_0000 == 0b0010_0000 {
                                if memory.irq48_signal == 0 {
                                    memory.request_interrupt(Interrupt::Lcd);
                                }
                                memory.irq48_signal |= 0b0000_0100;
                            }
                            memory.irq48_signal &= 0x0E;
                        }
                        self.update_stat_register(memory);
                    }
                }
                VBLANK => {
                    memory.gpu_cycles.aux_cycles_counter += cycles;

                    if memory.gpu_cycles.aux_cycles_counter >= 456 {
                        memory.gpu_cycles.aux_cycles_counter -= 456;
                        self.vblank_line += 1;

                        if self.vblank_line <= 9 {
                            memory.scan_line += 1;
                            memory.compare_ly_to_lyc();
                        }
                    }

                    if memory.gpu_cycles.cycles_counter >= 4104
                        && memory.gpu_cycles.aux_cycles_counter >= 4
                        && memory.scan_line == 153
                    {
                        memory.scan_line = 0;
                        memory.compare_ly_to_lyc();
                    }

                    if memory.gpu_cycles.cycles_counter >= 4560 {
                        memory.gpu_cycles.cycles_counter -= 4560;
                        memory.lcd_status_mode = (memory.lcd_status_mode & 0b1111_1100) | OAM_SCAN;
                        self.update_stat_register(memory);

                        memory.irq48_signal &= 0x0A;
                        let stat = memory.get_lcd_status_from_memory();
                        if stat & 0b0010_0000 == 0b0010_0000 {
                            if memory.irq48_signal == 0 {
                                memory.request_interrupt(Interrupt::Lcd);
                            }
                            memory.irq48_signal |= 0b0000_0100;
                        }
                        memory.irq48_signal &= 0x0D;
                    }
                }
                OAM_SCAN => {
                    if memory.gpu_cycles.cycles_counter >= 80 {
                        memory.gpu_cycles.cycles_counter -= 80;
                        memory.lcd_status_mode = (memory.lcd_status_mode & 0b1111_1100) | 0b11;
                        self.scan_line_transfered = false;
                        memory.irq48_signal &= 0x08;
                        self.update_stat_register(memory);
                    }
                }
                LCD_TRANSFER => {
                    if memory.gpu_cycles.pixel_counter < 160 {
                        self.tile_cycles_counter += cycles;

                        let lcdc =
                            LcdControlFlag::from_bits_truncate(memory.read_byte(LCD_CONTROL_INDEX));
                        if !memory.screen_disabled && lcdc.contains(LcdControlFlag::DISPLAY) {
                            while self.tile_cycles_counter >= 3 {
                                self.render_background(
                                    memory,
                                    memory.scan_line as i32,
                                    memory.gpu_cycles.pixel_counter,
                                    4,
                                    system,
                                );
                                memory.gpu_cycles.pixel_counter += 4;
                                self.tile_cycles_counter -= 3;

                                if memory.gpu_cycles.pixel_counter >= 160 {
                                    break;
                                }
                            }
                        }
                    }

                    if memory.gpu_cycles.cycles_counter >= 160 && !self.scan_line_transfered {
                        self.scan_line(memory, memory.scan_line as i32, system);
                        self.scan_line_transfered = true;
                    }

                    if memory.gpu_cycles.cycles_counter >= 172 {
                        memory.gpu_cycles.pixel_counter = 0;
                        memory.gpu_cycles.cycles_counter -= 172;
                        memory.lcd_status_mode = 0;
                        self.tile_cycles_counter = 0;
                        self.update_stat_register(memory);

                        memory.irq48_signal &= 0x08;
                        let stat = memory.get_lcd_status_from_memory();
                        if stat & 0b0000_1000 == 0b0000_1000 {
                            if memory.irq48_signal & 0b0000_1000 != 0b0000_1000 {
                                memory.request_interrupt(Interrupt::Lcd);
                            }
                            memory.irq48_signal |= 0b0000_0001;
                        }
                    }
                }
                _ => panic!("Impossible"),
            }
        } else {
            if memory.gpu_cycles.screen_enable_delay_cycles > 0 {
                memory.gpu_cycles.screen_enable_delay_cycles -= cycles;

                if memory.gpu_cycles.screen_enable_delay_cycles <= 0 {
                    memory.gpu_cycles.screen_enable_delay_cycles = 0;
                    memory.screen_disabled = false;
                    self.hide_frames = 3;
                    memory.lcd_status_mode = 0;
                    memory.gpu_cycles.cycles_counter = 0;
                    memory.gpu_cycles.aux_cycles_counter = 0;
                    memory.scan_line = 0;
                    memory.gpu_cycles.window_line = 0;
                    self.vblank_line = 0;
                    memory.gpu_cycles.pixel_counter = 0;
                    self.tile_cycles_counter = 0;
                    memory.irq48_signal = 0;

                    let stat = memory.get_lcd_status_from_memory();
                    if stat & 0b0010_0000 == 0b0010_0000 {
                        memory.request_interrupt(Interrupt::Lcd);
                        memory.irq48_signal |= 0b0000_0100;
                    }

                    memory.compare_ly_to_lyc();
                }
            } else if memory.gpu_cycles.cycles_counter >= 70224 {
                memory.gpu_cycles.cycles_counter -= 70224;
                vblank = true;
            }
        }
        vblank
    }

    fn update_stat_register(&self, memory: &mut Memory) {
        let stat = memory.read_byte(LCD_INDEX);
        memory.set_lcd_status_from_memory((stat & 0xFC) | (memory.lcd_status_mode & 0x3));
    }

    fn scan_line<T: PixelMapper>(&mut self, memory: &mut Memory, line: i32, system: &mut T) {
        let lcd_control = LcdControlFlag::from_bits_truncate(memory.read_byte(LCD_CONTROL_INDEX));
        if !memory.screen_disabled && lcd_control.contains(LcdControlFlag::DISPLAY) {
            self.render_window(memory, line, system);
            self.render_sprites(memory, line, system);
        } else {
            let line_width = (GAMEBOY_HEIGHT - 1 - line) * GAMEBOY_WIDTH;
            for x in 0..GAMEBOY_WIDTH {
                let index = (line_width + x) as usize;
                system.map_pixel(index, Color::White);
            }
        }
    }

    fn render_background<T: PixelMapper>(
        &mut self,
        memory: &Memory,
        line: i32,
        pixel: i32,
        count: i32,
        system: &mut T,
    ) {
        let offset_x_start = pixel % 8;
        let offset_x_end = offset_x_start + count;
        let screen_tile = pixel / 8;
        let lcd_control = LcdControlFlag::from_bits_truncate(memory.read_byte(LCD_CONTROL_INDEX));
        let line_width = (GAMEBOY_HEIGHT - 1 - line) * GAMEBOY_WIDTH;

        if lcd_control.contains(LcdControlFlag::DISPLAY) {
            let tile_start_addr = if lcd_control.contains(LcdControlFlag::BACKGROUND_TILE_SET) {
                0x8000
            } else {
                0x8800
            };

            let map_start_addr = if lcd_control.contains(LcdControlFlag::BACKGROUND_TILE_MAP) {
                0x9C00
            } else {
                0x9800
            };

            let scroll_x = memory.read_byte(SCROLL_X_INDEX);
            let scroll_y = memory.read_byte(SCROLL_Y_INDEX);
            let line_scrolled = scroll_y.wrapping_add(line as u8);
            let line_scrolled_32 = (line_scrolled as i32 / 8) * 32;
            let tile_pixel_y = (line_scrolled % 8) as i32;
            let tile_pixel_y_2 = tile_pixel_y * 2;

            for offset_x in offset_x_start..offset_x_end {
                let screen_pixel_x = (screen_tile * 8) + offset_x;
                let map_pixel_x = scroll_x.wrapping_add(screen_pixel_x as u8);
                let map_tile_x = (map_pixel_x / 8) as i32;
                let map_tile_offset_x = map_pixel_x % 8;
                let map_tile_addr = (map_start_addr + line_scrolled_32 + map_tile_x) as u16;

                let map_tile = if lcd_control.contains(LcdControlFlag::BACKGROUND_TILE_SET) {
                    memory.read_byte(map_tile_addr) as i32
                } else {
                    (memory.read_byte(map_tile_addr) as i8 as i32 + 128)
                };

                let map_tile_16 = map_tile * 16;
                let tile_address = (tile_start_addr + map_tile_16 + tile_pixel_y_2) as u16;
                let byte1 = memory.read_byte(tile_address);
                let byte2 = memory.read_byte(tile_address + 1);
                let pixel_x_in_tile = map_tile_offset_x as i32;
                let pixel_x_in_tile_bit = 0x1 << (7 - pixel_x_in_tile) as u8;

                let mut pixel_data = if byte1 & pixel_x_in_tile_bit != 0 {
                    1
                } else {
                    0
                };

                pixel_data |= if byte2 & pixel_x_in_tile_bit != 0 {
                    2
                } else {
                    0
                };

                let index = (line_width + screen_pixel_x) as usize;
                let palette = memory.read_byte(BACKGROUND_PALETTE_INDEX);
                let color_bits = (palette >> (pixel_data * 2)) & 0x03;
                let color = match color_bits {
                    0b00 => Color::White,
                    0b01 => Color::LightGray,
                    0b10 => Color::DarkGray,
                    0b11 => Color::Black,
                    _ => panic!("impossible"),
                };
                system.map_pixel(index, color);
            }
        } else {
            for x in 0..GAMEBOY_WIDTH {
                let index = (line_width + x) as usize;
                system.map_pixel(index, Color::White);
            }
        }
    }

    fn render_window<T: PixelMapper>(&mut self, memory: &mut Memory, line: i32, system: &mut T) {
        if memory.gpu_cycles.window_line > 143 {
            return;
        }

        let lcd_control = LcdControlFlag::from_bits_truncate(memory.read_byte(LCD_CONTROL_INDEX));

        if !lcd_control.contains(LcdControlFlag::WINDOW) {
            return;
        }

        let wx = memory.read_byte(WINDOW_X_INDEX) as i32 - 7;

        if wx > 159 {
            return;
        }

        let wy = memory.read_byte(WINDOW_Y_INDEX) as i32;

        if (wy > 143) || (wy > line) {
            return;
        }

        let tiles = if lcd_control.contains(LcdControlFlag::BACKGROUND_TILE_SET) {
            0x8000
        } else {
            0x8800
        };

        let map = if lcd_control.contains(LcdControlFlag::WINDOW_TILE_MAP) {
            0x9C00
        } else {
            0x9800
        };

        let line_adjusted = memory.gpu_cycles.window_line as i32;
        let y_32 = (line_adjusted / 8) * 32;
        let pixely = line_adjusted % 8;
        let pixely_2 = pixely * 2;
        let line_width = (GAMEBOY_HEIGHT - 1 - line) * GAMEBOY_WIDTH;

        for x in 0..32 {
            let tile = if lcd_control.contains(LcdControlFlag::BACKGROUND_TILE_SET) {
                memory.read_byte((map + y_32 + x) as u16) as i32
            } else {
                (memory.read_byte((map + y_32 + x) as u16) as i8 as i32 + 128)
            };

            let map_offset_x = x * 8;
            let tile_16 = tile * 16;
            let tile_address = (tiles + tile_16 + pixely_2) as u16;
            let byte1 = memory.read_byte(tile_address);
            let byte2 = memory.read_byte(tile_address + 1);

            for pixelx in 0..8 {
                let buffer_x = map_offset_x + pixelx + wx;

                if buffer_x < 0 || buffer_x >= GAMEBOY_WIDTH {
                    continue;
                }

                let pixelx_pos = pixelx as u8;

                let mut pixel = if (byte1 & (0x1 << (7 - pixelx_pos))) != 0 {
                    1
                } else {
                    0
                };

                pixel |= if (byte2 & (0x1 << (7 - pixelx_pos))) != 0 {
                    2
                } else {
                    1
                };

                let position = line_width + buffer_x;
                let palette = memory.read_byte(BACKGROUND_PALETTE_INDEX);
                let color_bits = (palette >> (pixel * 2)) & 0x03;
                let color = match color_bits {
                    0b00 => Color::White,
                    0b01 => Color::LightGray,
                    0b10 => Color::DarkGray,
                    0b11 => Color::Black,
                    _ => panic!("impossible"),
                };
                system.map_pixel(position as usize, color);
            }
        }
        memory.gpu_cycles.window_line += 1;
    }

    fn render_sprites<T: PixelMapper>(&mut self, memory: &Memory, line: i32, system: &mut T) {
        let lcd_control = LcdControlFlag::from_bits_truncate(memory.read_byte(LCD_CONTROL_INDEX));

        if !lcd_control.contains(LcdControlFlag::SPRITES) {
            return;
        }

        let sprite_height = if lcd_control.contains(LcdControlFlag::SPRITES_SIZE) {
            16
        } else {
            8
        };

        let line_width = (GAMEBOY_HEIGHT - 1 - line) * GAMEBOY_WIDTH;

        for sprite in (0..40).rev() {
            let sprite_4 = sprite * 4;
            let sprite_y = memory.read_byte(SPRITES_START_INDEX + sprite_4) as i32 - 16;

            if (sprite_y > line) || (sprite_y + sprite_height) <= line {
                continue;
            }

            let sprite_x = memory.read_byte(SPRITES_START_INDEX + sprite_4 + 1) as i32 - 8;

            if (sprite_x < -7) || (sprite_x >= GAMEBOY_WIDTH) {
                continue;
            }

            let sprite_tile_16 = if lcd_control.contains(LcdControlFlag::SPRITES_SIZE) {
                (memory.read_byte((SPRITES_START_INDEX + sprite_4 + 2) as u16) & 0xFE) as i32 * 16
            } else {
                (memory.read_byte((SPRITES_START_INDEX + sprite_4 + 2) as u16) & 0xFF) as i32 * 16
            };

            let sprite_flags = SpriteAttributes::from_bits_truncate(
                memory.read_byte((SPRITES_START_INDEX + sprite_4 + 3) as u16),
            );

            let sprite_pallette = sprite_flags.contains(SpriteAttributes::PALETTE);
            let xflip = sprite_flags.contains(SpriteAttributes::X_FLIP);
            let yflip = sprite_flags.contains(SpriteAttributes::Y_FLIP);
            let behind_bg = sprite_flags.contains(SpriteAttributes::BACKGROUND_PRIORITY);
            let tiles = 0x8000;

            let pixel_y = if yflip {
                let height = if lcd_control.contains(LcdControlFlag::SPRITES_SIZE) {
                    15
                } else {
                    7
                };
                height - (line - sprite_y)
            } else {
                line - sprite_y
            };

            let (pixel_y_2, offset) =
                if lcd_control.contains(LcdControlFlag::SPRITES_SIZE) && pixel_y >= 8 {
                    ((pixel_y - 8) * 2, 16)
                } else {
                    (pixel_y * 2, 0)
                };

            let tile_address = tiles + sprite_tile_16 + pixel_y_2 + offset;

            let byte1 = memory.read_byte(tile_address as u16);
            let byte2 = memory.read_byte(tile_address as u16 + 1);

            for pixelx in 0..8 {
                let mut pixel = if xflip {
                    if byte1 & (0x01 << pixelx) != 0 {
                        1
                    } else {
                        0
                    }
                } else {
                    if byte1 & (0x01 << (7 - pixelx)) != 0 {
                        1
                    } else {
                        0
                    }
                };

                pixel |= if xflip {
                    if byte2 & (0x01 << pixelx) != 0 {
                        2
                    } else {
                        0
                    }
                } else {
                    if byte2 & (0x01 << (7 - pixelx)) != 0 {
                        2
                    } else {
                        0
                    }
                };

                if pixel == 0 {
                    continue;
                }

                let buffer_x = sprite_x + pixelx as i32;

                if buffer_x < 0 || buffer_x >= GAMEBOY_WIDTH {
                    continue;
                }

                let position = line_width + buffer_x;

                // the background should take priorify if the color isn't white
                if behind_bg && system.get_pixel(position as usize) != Color::White {
                    continue;
                }

                let palette = if sprite_pallette {
                    memory.read_byte(OBJECT_PALETTE_1_INDEX)
                } else {
                    memory.read_byte(OBJECT_PALETTE_0_INDEX)
                };
                let color_bits = (palette >> (pixel * 2)) & 0x03;
                let color = match color_bits {
                    0b00 => Color::White,
                    0b01 => Color::LightGray,
                    0b10 => Color::DarkGray,
                    0b11 => Color::Black,
                    _ => panic!("impossible"),
                };
                system.map_pixel(position as usize, color);
            }
        }
    }
}
