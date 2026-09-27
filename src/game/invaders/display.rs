use std::cell::RefCell;
use std::rc::Rc;
use std::time::{SystemTime, UNIX_EPOCH};

use minifb::{Key, Scale, ScaleMode, Window, WindowOptions};

pub struct Display {
    window: Window,
    buffer: Vec<u32>,
    video_arr: Rc<RefCell<Vec<u8>>>,
}

const GAME_NAME: &str = "Space Invaders";
const WIDTH: usize = 224;
const HEIGHT: usize = 256;

impl Display {
    pub fn new(video_arr: Rc<RefCell<Vec<u8>>>) -> Self {
        let window = Window::new(
            format!("{} - Powered by Jelipo", GAME_NAME).as_str(),
            WIDTH,
            HEIGHT,
            WindowOptions {
                borderless: false,
                transparency: false,
                title: true,
                resize: false,
                scale: Scale::X2,
                scale_mode: ScaleMode::Stretch,
                topmost: false,
                none: false,
            },
        )
        .unwrap_or_else(|e| {
            panic!("{}", e);
        });
        Self {
            window,
            buffer: vec![0; WIDTH * HEIGHT],
            video_arr,
        }
    }

    /// This is block method
    pub fn start(&mut self) {
        // 限制最高60帧
        self.window.set_target_fps(60);

        let mut lasttime = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
        while self.window.is_open() && !self.window.is_key_down(Key::Escape) {
            lasttime = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
            self.set_buffer(self.video_arr.clone());
            self.window
                .update_with_buffer(&self.buffer, WIDTH, HEIGHT)
                .unwrap();
        }
    }

    pub fn update_cycle(&mut self) -> Option<Key> {
        self.set_buffer(self.video_arr.clone());
        self.window
            .update_with_buffer(&self.buffer, WIDTH, HEIGHT)
            .unwrap();
        return if self.window.is_key_down(Key::Left) {
            Some(Key::Left)
        } else if self.window.is_key_down(Key::Right) {
            Some(Key::Right)
        } else if self.window.is_key_down(Key::Space) {
            Some(Key::Space)
        } else if self.window.is_key_down(Key::Enter) {
            Some(Key::Enter)
        } else if self.window.is_key_down(Key::C) {
            Some(Key::C)
        } else {
            None
        };
    }

    fn set_buffer(&mut self, video_arr: Rc<RefCell<Vec<u8>>>) {
        let gpu_ram = video_arr.borrow();
        for i in 0..gpu_ram.len() {
            let gpu_byte = gpu_ram[i];
            // display_point
            let dp = i * 8;
            let _buffer_size = self.buffer.len();
            self.set_point(dp, get_color(gpu_byte & 0b0000_0001));
            self.set_point(dp + 1, get_color(gpu_byte & 0b0000_0010));
            self.set_point(dp + 2, get_color(gpu_byte & 0b0000_0100));
            self.set_point(dp + 3, get_color(gpu_byte & 0b0000_1000));
            self.set_point(dp + 4, get_color(gpu_byte & 0b0001_0000));
            self.set_point(dp + 5, get_color(gpu_byte & 0b0010_0000));
            self.set_point(dp + 6, get_color(gpu_byte & 0b0100_0000));
            self.set_point(dp + 7, get_color(gpu_byte & 0b1000_0000));
        }
    }

    fn set_point(&mut self, display_point: usize, color: u32) {
        let new_x = display_point / HEIGHT;
        let new_y = HEIGHT - 1 - (display_point % HEIGHT);
        self.buffer[new_y * WIDTH + new_x] = color;
    }
}

fn get_color(bit: u8) -> u32 {
    return if bit == 0 { 0 } else { u32::MAX };
}
