use std::cell::RefCell;
use std::fs::File;
use std::io::Read;
use std::rc::Rc;

use std::time::{Duration, SystemTime, UNIX_EPOCH};
use std::{io, thread};

use crate::cpu::Cpu;
use crate::game::invaders::display::Display;
use crate::game::invaders::gameio::InvadersIO;
use crate::game::invaders::InvadersAddressBus;
use crate::game::Launch;

pub struct InvadersLaunch {}

impl Launch for InvadersLaunch {
    fn start(&self) {
        let gpu_ram = vec![0u8; 7168];

        let video_arr = Rc::new(RefCell::new(gpu_ram));
        let video_arr_cloned = video_arr.clone();

        let addressing = init_address(video_arr_cloned).unwrap();

        let io = Rc::new(RefCell::new(InvadersIO::new()));
        let mut cpu = Cpu::new(Box::new(addressing), 0, io.clone());
        let mut int_num: bool = false;
        let mut time = get_mill_time();
        let mut int_times = 0;
        let cycle_max: u32 = 17476;
        let _max_fps: u8 = 60;
        let mut fps_temp: u8 = 0;
        let mut fps_timelinei128 = get_mill_time();
        let mut video = Display::new(video_arr);
        //video.start();
        let loop_io = io;
        loop {
            let mut cycle_temp: u32 = 0;
            loop {
                let cycle = cpu.next();
                cycle_temp += cycle as u32;
                if cycle_temp > cycle_max {
                    cycle_temp = 0;
                    break;
                }
            }
            let result = cpu.interrupt(if int_num { 0x10 } else { 0x08 });
            if result {
                int_num = !int_num;
                int_times += 1;
                if (get_mill_time() - time) > 10000 {
                    println!("10 sec : {} fps", int_times);
                    time = get_mill_time();
                    int_times = 0;
                }
            }
            if result {
                loop {
                    let cycle = cpu.next();
                    cycle_temp += cycle as u32;
                    if cycle_temp > cycle_max {
                        cycle_temp = 0;
                        break;
                    }
                }
                let keys = video.update_cycle();
                loop_io.borrow_mut().set_input_temp(keys);

                fps_temp += 1;
                let time_now = get_mill_time();

                let i = (time_now - fps_timelinei128) as u16;
                if fps_temp >= 60 {
                    if i < 1000 {
                        let sleep = 1000 - i;
                        println!("补充睡眠 {}ms", sleep);
                        thread::sleep(Duration::from_millis(sleep as u64));
                    }
                    println!("重置 {}", time_now);
                    fps_temp = 0;
                    fps_timelinei128 = time_now;
                } else {
                    let sleep = (1000_u16.saturating_sub(i)) / (60 - fps_temp) as u16;
                    if sleep != 0 {
                        //println!("睡眠 {}ms", sleep);
                        thread::sleep(Duration::from_millis(sleep as u64));
                    }
                }
            } else {
                println!("not")
            }
        }
    }
}

fn get_mill_time() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis()
}

fn get_micro_time() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_micros()
}

impl InvadersLaunch {
    pub fn new() -> Self {
        Self {}
    }
}

fn init_address(video_arr: Rc<RefCell<Vec<u8>>>) -> io::Result<InvadersAddressBus> {
    let mut arr_h = [0u8; 2048];
    let mut h = File::open("./res/invaders.h")?;
    h.read_exact(&mut arr_h)?;

    let mut arr_g = [0u8; 2048];
    let mut g = File::open("./res/invaders.g")?;
    g.read_exact(&mut arr_g)?;

    let mut arr_f = [0u8; 2048];
    let mut f = File::open("./res/invaders.f")?;
    f.read_exact(&mut arr_f)?;

    let mut arr_e = [0u8; 2048];
    let mut e = File::open("./res/invaders.e")?;
    e.read_exact(&mut arr_e)?;

    let addressing = InvadersAddressBus::new(
        Box::new(arr_h),
        Box::new(arr_g),
        Box::new(arr_f),
        Box::new(arr_e),
        video_arr,
    );
    Ok(addressing)
}
