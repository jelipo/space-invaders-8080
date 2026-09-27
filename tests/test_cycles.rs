use std::cell::RefCell;
use std::rc::Rc;

use space_invaders_8080::{cpu::IO, Cpu, Register, TestAddressing};

struct NoIO;

impl IO for NoIO {
    fn input(&mut self, _: &mut Register, _: u8) {}
    fn output(&mut self, _: &mut Cpu, _: u8) {}
}

fn cpu_with_opcode(opcode: u8) -> Cpu {
    let mut memory = vec![0u8; 65536];
    memory[..3].copy_from_slice(&[opcode, 0x34, 0x12]);
    memory[0x2000..0x2002].copy_from_slice(&[0x78, 0x56]);
    let mut cpu = Cpu::new(
        Box::new(TestAddressing::new(Rc::new(RefCell::new(memory)))),
        0,
        Rc::new(RefCell::new(NoIO)),
    );
    cpu.register.sp = 0x2000;
    cpu
}

// Return, jump, call, flag mask, flag value required for the condition.
const CONDITIONS: [(u8, u8, u8, u8, bool); 8] = [
    (0xc0, 0xc2, 0xc4, 0x40, false), // NZ
    (0xc8, 0xca, 0xcc, 0x40, true),  // Z
    (0xd0, 0xd2, 0xd4, 0x01, false), // NC
    (0xd8, 0xda, 0xdc, 0x01, true),  // C
    (0xe0, 0xe2, 0xe4, 0x04, false), // PO
    (0xe8, 0xea, 0xec, 0x04, true),  // PE
    (0xf0, 0xf2, 0xf4, 0x80, false), // P
    (0xf8, 0xfa, 0xfc, 0x80, true),  // M
];

#[test]
fn fixed_cycles_have_no_surcharge() {
    for (opcode, cycles) in [
        (0x00, 4),  // NOP
        (0x41, 5),  // MOV B,C
        (0x46, 7),  // MOV B,M
        (0x80, 4),  // ADD B
        (0xc3, 10), // JMP
        (0xc5, 11), // PUSH B
        (0xcd, 17), // CALL
        (0xc9, 10), // RET
        (0xcf, 11), // RST 1
    ] {
        assert_eq!(
            cpu_with_opcode(opcode).next(),
            cycles,
            "opcode {opcode:02x}"
        );
    }
}

#[test]
fn conditional_jumps_always_take_ten_cycles() {
    for (_, opcode, _, mask, required) in CONDITIONS {
        for taken in [false, true] {
            let mut cpu = cpu_with_opcode(opcode);
            cpu.register
                .set_flags(if taken == required { mask } else { 0 });
            assert_eq!(cpu.next(), 10, "opcode {opcode:02x}, taken={taken}");
            assert_eq!(cpu.register.pc, if taken { 0x1234 } else { 3 });
            assert_eq!(cpu.register.sp, 0x2000);
        }
    }
}

#[test]
fn conditional_calls_take_eleven_or_seventeen_cycles() {
    for (_, _, opcode, mask, required) in CONDITIONS {
        for taken in [false, true] {
            let mut cpu = cpu_with_opcode(opcode);
            cpu.register
                .set_flags(if taken == required { mask } else { 0 });
            assert_eq!(
                cpu.next(),
                if taken { 17 } else { 11 },
                "opcode {opcode:02x}, taken={taken}"
            );
            assert_eq!(cpu.register.pc, if taken { 0x1234 } else { 3 });
            assert_eq!(cpu.register.sp, if taken { 0x1ffe } else { 0x2000 });
            if taken {
                assert_eq!(cpu.addring.get_word(0x1ffe), 3);
            }
        }
    }
}

#[test]
fn conditional_returns_take_five_or_eleven_cycles() {
    for (opcode, _, _, mask, required) in CONDITIONS {
        for taken in [false, true] {
            let mut cpu = cpu_with_opcode(opcode);
            cpu.register
                .set_flags(if taken == required { mask } else { 0 });
            assert_eq!(
                cpu.next(),
                if taken { 11 } else { 5 },
                "opcode {opcode:02x}, taken={taken}"
            );
            assert_eq!(cpu.register.pc, if taken { 0x5678 } else { 1 });
            assert_eq!(cpu.register.sp, if taken { 0x2002 } else { 0x2000 });
        }
    }
}
