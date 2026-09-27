use std::cell::RefCell;
use std::rc::Rc;

use space_invaders_8080::{cpu::IO, Cpu, Register, TestAddressing};

struct NoIO;

impl IO for NoIO {
    fn input(&mut self, _: &mut Register, _: u8) {}

    fn output(&mut self, _: &mut Cpu, _: u8) {}
}

#[test]
fn jp_branches_when_sign_is_clear_regardless_of_parity() {
    for sign in [false, true] {
        for parity in [false, true] {
            let mut memory = vec![0u8; 65536];
            memory[..3].copy_from_slice(&[0xf2, 0x34, 0x12]); // JP 1234h
            let mut cpu = Cpu::new(
                Box::new(TestAddressing::new(Rc::new(RefCell::new(memory)))),
                0,
                Rc::new(RefCell::new(NoIO)),
            );
            cpu.register.flag_s = sign;
            cpu.register.flag_p = parity;
            let flags = cpu.register.get_flags();

            cpu.next();

            assert_eq!(cpu.register.pc, if sign { 3 } else { 0x1234 });
            assert_eq!(cpu.register.get_flags(), flags);
        }
    }
}
