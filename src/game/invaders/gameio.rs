use minifb::Key;

use crate::cpu::Register;
use crate::cpu::{Cpu, IO};

pub struct InvadersIO {
    input_temp: Vec<Key>,
}

impl InvadersIO {
    pub fn new() -> Self {
        Self { input_temp: Vec::new() }
    }

    pub fn set_input_temp(&mut self, keys: Vec<Key>) {
        self.input_temp = keys;
    }
}

impl IO for InvadersIO {
    fn input(&mut self, cpu: &mut Register, byte: u8) {
        // 先设置端口默认值，再叠加按键状态。
        match byte {
            1 => cpu.a = 0b0000_1000,
            2 => cpu.a = 0b1000_0000,
            _ => {}
        }
        if byte == 1 {
            for key in &self.input_temp {
                cpu.a |= match key {
                    Key::C => 0b0000_0001,
                    Key::Enter => 0b0000_0100,
                    Key::Space => 0b0001_0000,
                    Key::Left => 0b0010_0000,
                    Key::Right => 0b0100_0000,
                    _ => 0,
                };
            }
        }
        //println!("执行input {:X}", byte);
    }

    fn output(&mut self, _cpu: &mut Cpu, _byte: u8) {
        println!("执行output");
    }
}
