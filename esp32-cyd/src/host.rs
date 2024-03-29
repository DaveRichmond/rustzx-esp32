use embedded_graphics::pixelcolor::{Rgb565, RgbColor};
use emulator::{io::FileAsset, stopwatch::InstantStopwatch};
use rustzx_core::{
    host::{FrameBuffer, FrameBufferSource, Host, HostContext, StubIoExtender}, 
    zx::video::colors::{ ZXBrightness, ZXColor},
};

use alloc::{vec, vec::Vec};

use log::info;

const LCD_H_RES : usize = 256;
const LCD_W_RES : usize = 192;
const LCD_PIXELS : usize = LCD_H_RES * LCD_W_RES;

pub(crate) struct Esp32Host {

}

impl Host for Esp32Host {
    type Context = Esp32HostContext;
    type EmulationStopwatch = InstantStopwatch;
    type FrameBuffer = EmbeddedGraphicsFrameBuffer;
    type TapeAsset = FileAsset;
    type IoExtender = StubIoExtender;
}

pub(crate) struct Esp32HostContext;
impl HostContext<Esp32Host> for Esp32HostContext {
    fn frame_buffer_context(&self) -> <<Esp32Host as Host>::FrameBuffer as FrameBuffer>::Context {
        ()
    }
}

pub(crate) struct EmbeddedGraphicsFrameBuffer {
    buffer: Vec<Rgb565>,
    buffer_width: usize,
    pub bounding_box_top_left : Option<(usize, usize)>,
    pub bounding_box_bottom_right : Option<(usize, usize)>,
}
use crate::colour_conv;

impl EmbeddedGraphicsFrameBuffer {
    fn mark_dirty(&mut self, x : usize, y : usize){
        let (min_x, min_y) = self.bounding_box_top_left.unwrap_or((x, y));
        let (max_x, max_y) = self.bounding_box_bottom_right.unwrap_or((x, y));
        self.bounding_box_top_left = Some((min_x.min(x), min_y.min(y)));
        self.bounding_box_bottom_right = Some((max_x.max(x), max_y.max(y)));
    }

    // pub fn get_region_pixel_iter(&self, top_left: (usize, usize), bottom_right: (usize, usize)) -> impl Iterator<Item = Rgb565> + '_ {
    //     let start_x = top_left.0;
    //     let end_x = bottom_right.0 + 1;
    //     let start_y =  top_left.1;
    //     let end_y = bottom_right.1 + 1;

    //     (start_y..end_y).flat_map(move |y| {
    //         (start_x..end_x).flat_map(move |x| {
    //             self.buffer[y * self.buffer_width + x]
    //         })
    //     })
    // }

    pub fn get_region_pixel_iter(&self, top_left: (usize, usize), bottom_right: (usize, usize)) -> impl Iterator<Item = Rgb565> + '_ {
        let start_x = top_left.0;
        let end_x = bottom_right.0 + 1; // Include the pixel at bottom_right coordinates
        let start_y = top_left.1;
        let end_y = bottom_right.1 + 1; // Include the pixel at bottom_right coordinates

        (start_y..end_y).flat_map(move |y| {
            (start_x..end_x).map(move |x| {
                self.buffer[y * self.buffer_width + x]
            })
        })
    }
}

impl FrameBuffer for EmbeddedGraphicsFrameBuffer {
    type Context = ();

    fn new(
        width: usize,
        height: usize,
        source: FrameBufferSource,
        _context: Self::Context
    ) -> Self {
        info!("Allocation");

        match source {
            FrameBufferSource::Screen => {
                info!("Allocating frame buffer width={}, height={}", width, height);

                Self {
                    buffer: vec![Rgb565::RED; LCD_PIXELS],
                    buffer_width: LCD_H_RES as usize,
                    bounding_box_bottom_right: None,
                    bounding_box_top_left: None,
                }
            }
            FrameBufferSource::Border => {
                info!("Allocating border");

                Self {
                    buffer: vec![Rgb565::WHITE; 1],
                    buffer_width: 1,
                    bounding_box_bottom_right: None,
                    bounding_box_top_left: None
                }
            }
        }
    }

    fn set_color(&mut self, x: usize, y: usize, color: ZXColor, brightness: ZXBrightness) {
        let index = y * self.buffer_width + x;
        let new_colour = colour_conv(&color, brightness);
        if self.buffer[index] != new_colour {
            self.buffer[index] = new_colour;
            self.mark_dirty(x, y);
        }
    }

    fn set_colors(&mut self, x: usize, y: usize, colours: [ZXColor; 8], brightness: ZXBrightness){
        for (i, &colour) in colours.iter().enumerate() {
            self.set_color(x + i, y, colour, brightness)
        }
    }

    fn reset_bounding_box(&mut self) {
        self.bounding_box_bottom_right = None;
        self.bounding_box_top_left = None;
    }
}
