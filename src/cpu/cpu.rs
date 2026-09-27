use std::cell::RefCell;
use std::mem;
use std::rc::Rc;

use crate::cpu::register::Register;
use crate::cpu::IO;
use crate::memory::address::AddressBus;
use crate::util::U16Util;

/// Abstraction of Intel 8080
pub struct Cpu {
    pub register: Register,
    pub addring: Box<dyn AddressBus>,
    interrupt: bool,
    io: Rc<RefCell<dyn IO>>,
}

impl Cpu {
    ///
    /// # Arguments
    /// * `name` - A string slice that holds the name of the person
    pub fn new(addring: Box<dyn AddressBus>, pc: u16, io: Rc<RefCell<dyn IO>>) -> Self {
        let register = Register {
            a: 0,
            b: 0,
            c: 0,
            d: 0,
            e: 0,
            h: 0,
            l: 0,
            pc,
            sp: 0,
            flag_z: false,
            flag_s: false,
            flag_p: false,
            flag_cy: false,
            flag_ac: false,
        };
        Self {
            register,
            addring,
            interrupt: false,
            io,
        }
    }

    fn get_next_byte(&mut self) -> u8 {
        let byte = self.addring.get_mem(self.register.pc);
        self.register.pc += 1;
        byte
    }

    fn get_next_word(&mut self) -> u16 {
        let addr = self.register.pc;
        let word =
            U16Util::from_le_bytes(self.addring.get_mem(addr), self.addring.get_mem(addr + 1));
        self.register.pc += 2;
        word
    }

    /// OPCODE: INR
    fn inr_add(&mut self, r: u8) -> u8 {
        let new_r = r.wrapping_add(1);
        self.register.flag_z = new_r == 0;
        self.register.flag_s = (new_r & 0b10000000) != 0;
        self.register.flag_p = new_r.count_ones() % 2 == 0x00;
        self.register.flag_ac = (r & 0x0f) + 0x01 > 0x0f;
        new_r
    }

    /// OPCODE: DCR
    fn dcr_sub(&mut self, r: u8) -> u8 {
        let new_r = r.wrapping_sub(1);
        self.register.flag_z = new_r == 0;
        self.register.flag_s = (new_r & 0b10000000) != 0;
        self.register.flag_p = new_r.count_ones() % 2 == 0x00;
        self.register.flag_ac = new_r > 0xf;
        new_r
    }

    fn daa(&mut self) {
        let mut a: u8 = 0;
        let mut c = self.register.flag_cy;
        let lsb = self.register.a & 0x0f;
        let msb = self.register.a >> 4;
        // If the least significant four bits of the accumulator represents a number greater than 9, or if the Auxiliary
        // Carry bit is equal to one, the accumulator is incremented by six. Otherwise, no incrementing occurs.
        if (lsb > 9) || self.register.flag_ac {
            a += 0x06;
        }
        // If the most significant four bits of the accumulator now represent a number greater than 9, or if the normal
        // carry bit is equal to one, the most sign ificant four bits of the accumulator are incremented by six.
        if (msb > 9) || self.register.flag_cy || (msb >= 9 && lsb > 9) {
            a += 0x60;
            c = true;
        }
        self.add(a);
        self.register.flag_cy = c;
    }

    /// r: maybe BC/
    fn dad_add(&mut self, r: u16) {
        let old_hl = self.register.get_hl();
        let new_hl = old_hl.wrapping_add(r);
        self.register.set_hl(new_hl);
        self.register.flag_cy = new_hl < old_hl;
    }

    fn add(&mut self, r: u8) {
        let r_a = self.register.a;
        let new_a = r_a.wrapping_add(r);
        self.register.a = new_a;
        self.register.flag_z = new_a == 0;
        self.register.flag_s = (new_a & 0b10000000) != 0;
        self.register.flag_p = new_a.count_ones() % 2 == 0x00;
        self.register.flag_cy = new_a < r_a;
        self.register.flag_ac = (r_a & 0x0f) + (r & 0x0f) > 0x0f;
    }

    /// example :  ADC B        1    Z, S, P, CY, AC    A <- A + B + CY
    fn adc(&mut self, r: u8) {
        let c = u8::from(self.register.flag_cy);
        let old_a = self.register.a;
        let old_cy: u8 = u8::from(self.register.flag_cy);
        let new_a = old_a.wrapping_add(r).wrapping_add(old_cy);
        self.register.flag_z = new_a == 0;
        self.register.flag_s = (new_a & 0b10000000) != 0;
        self.register.flag_p = new_a.count_ones() % 2 == 0x00;
        self.register.flag_cy = u16::from(old_a) + u16::from(r) + u16::from(old_cy) > 0xff;
        self.register.flag_ac = (old_a & 0x0f) + (r & 0x0f) + c > 0x0f;
        self.register.a = new_a;
    }

    /// example : SUB B        1    Z, S, P, CY, AC    A <- A - B
    fn sub(&mut self, r: u8) {
        let old_a = self.register.a;
        let new_a = old_a.wrapping_sub(r);
        self.register.flag_z = new_a == 0;
        self.register.flag_s = (new_a & 0b10000000) != 0;
        self.register.flag_p = new_a.count_ones() % 2 == 0x00;
        self.register.flag_cy = old_a < new_a;
        self.register.flag_ac = (old_a as i8 & 0x0f) - (r as i8 & 0x0f) >= 0x00;
        self.register.a = new_a
    }

    ///
    /// example : SBB B        1    Z, S, P, CY, AC    A <- A - B - CY
    fn sbb(&mut self, r: u8) {
        let c = u8::from(self.register.flag_cy);
        let old_a = self.register.a;
        let old_cy = u8::from(self.register.flag_cy);
        let new_a = old_a.wrapping_sub(r).wrapping_sub(old_cy);
        self.register.flag_z = new_a == 0;
        self.register.flag_s = (new_a & 0b10000000) != 0;
        self.register.flag_p = new_a.count_ones() % 2 == 0x00;
        self.register.flag_cy = u16::from(old_a) < (u16::from(r) + u16::from(old_cy));
        self.register.flag_ac = (old_a as i8 & 0x0f) - (r as i8 & 0x0f) - (c as i8) >= 0x00;
        self.register.a = new_a;
    }

    /// ANA opcode
    /// example : ANA B        1    Z, S, P, CY, AC    A <- A & B
    fn ana(&mut self, r: u8) {
        let new_a = self.register.a & r;
        self.register.flag_z = new_a == 0;
        self.register.flag_s = (new_a & 0b10000000) != 0;
        self.register.flag_p = new_a.count_ones() % 2 == 0x00;
        self.register.flag_cy = false;
        self.register.flag_ac = ((self.register.a & r) & 0x08) != 0;
        self.register.a = new_a;
    }

    ///
    /// example : XRA B        1    Z, S, P, CY, AC    A <- A ^ B
    fn xra(&mut self, r: u8) {
        let new_a = self.register.a ^ r;
        self.register.flag_z = new_a == 0;
        self.register.flag_s = (new_a & 0b10000000) != 0;
        self.register.flag_p = new_a.count_ones() % 2 == 0x00;
        self.register.flag_cy = false;
        self.register.flag_ac = false;
        self.register.a = new_a;
    }

    ///
    /// example : ORA B        1    Z, S, P, CY, AC    A <- A | B
    fn ora(&mut self, r: u8) {
        let new_a = self.register.a | r;
        self.register.flag_z = new_a == 0;
        self.register.flag_s = (new_a & 0b10000000) != 0;
        self.register.flag_p = new_a.count_ones() % 2 == 0x00;
        self.register.flag_cy = false;
        self.register.flag_ac = false;
        self.register.a = new_a;
    }

    ///
    /// example : CMP B        1    Z, S, P, CY, AC    A - B
    fn cmp(&mut self, r: u8) {
        let old_a = self.register.a;
        let new_a = old_a.wrapping_sub(r);
        self.register.flag_z = new_a == 0x00;
        self.register.flag_s = (new_a & 0b10000000) != 0;
        self.register.flag_p = new_a.count_ones() % 2 == 0x00;
        self.register.flag_cy = old_a < new_a;
        self.register.flag_ac = ((old_a as i8 & 0x0f) - (r as i8 & 0x0f)) >= 0x00;
    }

    /// Add value to Stack
    fn stack_add(&mut self, value: u16) {
        self.register.sp = self.register.sp.wrapping_sub(2);
        self.addring.set_word(self.register.sp, value);
    }

    /// Pop value from Stack
    fn stack_pop(&mut self) -> u16 {
        let value = self.addring.get_word(self.register.sp);
        self.register.sp = self.register.sp.wrapping_add(2);
        value
    }

    fn jmp(&mut self) {
        self.register.pc = self.get_next_word();
    }

    /// 根据跳转判断是否做 JMP 操作
    fn condition_jmp(&mut self, condition: bool) {
        let word = self.get_next_word();
        if condition {
            self.register.pc = word;
        }
    }

    /// 根据跳转判断是否做 CALL 操作
    fn condition_call(&mut self, condition: bool) -> bool {
        let word = self.get_next_word();
        if condition {
            self.stack_add(self.register.pc);
            self.register.pc = word;
        }
        condition
    }

    fn call(&mut self) {
        let word = self.get_next_word();
        self.stack_add(self.register.pc);
        self.register.pc = word;
    }

    fn rst(&mut self, opcode: u8) {
        self.stack_add(self.register.pc);
        self.register.pc = u16::from(opcode & 0x38);
    }

    /// 下一步指令
    pub fn next(&mut self) -> u8 {
        let op_code = self.get_next_byte();
        // 条件 CALL/RET 实际执行时才额外增加 6 个周期。
        let mut ex_cycle: bool = false;
        match op_code {
            // NOP          1
            0x00 => { /* Nothing */ }
            // LXI B,D16    3                      B <- byte 3, C <- byte 2
            0x01 => {
                self.register.c = self.get_next_byte();
                self.register.b = self.get_next_byte();
            }
            // STAX B       1                      (BC) <- A
            0x02 => self
                .addring
                .set_mem(self.register.get_bc(), self.register.a),
            // INX B        1                      BC <- BC+1
            0x03 => self.register.set_bc(self.register.get_bc().wrapping_add(1)),
            // INR B        1    Z, S, P, AC       B <- B+1
            0x04 => self.register.b = self.inr_add(self.register.b),
            // DCR B        1    Z, S, P, AC       B <- B-1
            0x05 => self.register.b = self.dcr_sub(self.register.b),
            // MVI B, D8    2                      B <- byte 2
            0x06 => self.register.b = self.get_next_byte(),
            // RLC          1    CY                A = A << 1; bit 0 = prev bit 7; CY = prev bit 7
            0x07 => {
                self.register.a = self.register.a.rotate_left(1);
                self.register.flag_cy = (self.register.a & 1) != 0;
            }
            // -
            0x08 => {
                eprintln!("未实现 {:#04X}", op_code);
                /* Nothing */
            }
            // DAD B        1    CY                HL = HL + BC
            0x09 => self.dad_add(self.register.get_bc()),
            // LDAX B       1                      A <- (BC)
            0x0a => self.register.a = self.addring.get_mem(self.register.get_bc()),
            // DCX B        1                      BC = BC-1
            0x0b => self.register.set_bc(self.register.get_bc().wrapping_sub(1)),
            // INR C        1    Z, S, P, AC       C <- C+1
            0x0c => self.register.c = self.inr_add(self.register.c),
            // DCR C        1    Z, S, P, AC       C <-C-1
            0x0d => self.register.c = self.dcr_sub(self.register.c),
            // MVI C,D8     2                      C <- byte 2
            0x0e => self.register.c = self.get_next_byte(),
            // RRC          1    CY                A = A >> 1; bit 7 = prev bit 0; CY = prev bit 0
            0x0f => {
                self.register.a = self.register.a.rotate_right(1);
                self.register.flag_cy = (self.register.a & 0x80) != 0;
            }
            // -
            0x10 => {
                eprintln!("未实现 {:#04X}", op_code);
                /* Nothing */
            }
            // LXI D,D16    3                      D <- byte 3, E <- byte 2
            0x11 => {
                let word = self.get_next_word();
                self.register.set_de(word);
            }
            // STAX D       1                      (DE) <- A
            0x12 => self
                .addring
                .set_mem(self.register.get_de(), self.register.a),
            // INX D        1                      DE <- DE + 1
            0x13 => self.register.set_de(self.register.get_de().wrapping_add(1)),
            // INR D        1    Z, S, P, AC       D <- D+1
            0x14 => self.register.d = self.inr_add(self.register.d),
            // DCR D        1    Z, S, P, AC       D <- D-1
            0x15 => self.register.d = self.dcr_sub(self.register.d),
            // MVI D, D8    2                      D <- byte 2
            0x16 => self.register.d = self.get_next_byte(),
            // RAL          1    CY                A = A << 1; bit 0 = prev CY; CY = prev bit 7
            0x17 => {
                let new_flag_cy = self.register.a & 0x80 != 0;
                self.register.a = (self.register.a << 1) | self.register.flag_cy as u8;
                self.register.flag_cy = new_flag_cy;
            }
            // -
            0x18 => { /* Nothing */ }
            // DAD D        1    CY                HL = HL + DE
            0x19 => self.dad_add(self.register.get_de()),
            // LDAX D       1                      A <- (DE)
            0x1a => self.register.a = self.addring.get_mem(self.register.get_de()),
            // DCX D        1                      DE = DE-1
            0x1b => self.register.set_de(self.register.get_de().wrapping_sub(1)),
            // INR E        1    Z, S, P, AC       E <-E+1
            0x1c => self.register.e = self.inr_add(self.register.e),
            // DCR E        1    Z, S, P, AC       E <- E-1
            0x1d => self.register.e = self.dcr_sub(self.register.e),
            // MVI E,D8     2                      E <- byte 2
            0x1e => self.register.e = self.get_next_byte(),
            // RAR          1    CY                A = A >> 1; bit 7 = prev bit 7; CY = prev bit 0
            0x1f => {
                let new_flag_cy = self.register.a & 1 != 0;
                self.register.a = (self.register.a >> 1) | ((self.register.flag_cy as u8) << 7);
                self.register.flag_cy = new_flag_cy;
            }
            // RIM          1                      special    TODO Space dont need
            0x20 => { /* Nothing */ }
            // LXI H,D16    3                      H <- byte 3, L <- byte 2
            0x21 => {
                let word = self.get_next_word();
                self.register.set_hl(word);
            }
            // SHLD adr     3                      (adr) <-L; (adr+1)<-H
            0x22 => {
                let addr = self.get_next_word();
                self.addring.set_word(addr, self.register.get_hl());
            }
            // INX H        1                      HL <- HL + 1
            0x23 => self.register.set_hl(self.register.get_hl().wrapping_add(1)),
            // INR H        1    Z, S, P, AC       H <- H+1
            0x24 => self.register.h = self.inr_add(self.register.h),
            // DCR H        1    Z, S, P, AC       H <- H-1
            0x25 => self.register.h = self.dcr_sub(self.register.h),
            // MVI H,D8     2                      H <- byte 2
            0x26 => self.register.h = self.get_next_byte(),
            // DAA          1                      special
            0x27 => {
                self.daa();
            }
            // -
            0x28 => { /* Nothing */ }
            // DAD H        1    CY                HL = HL + HI
            0x29 => self.dad_add(self.register.get_hl()),
            // LHLD adr     3                      L <- (adr); H<-(adr+1)
            0x2a => {
                let addr = self.get_next_word();
                let word = self.addring.get_word(addr);
                self.register.set_hl(word);
            }
            // DCX H        1                      HL = HL-1
            0x2b => self.register.set_hl(self.register.get_hl().wrapping_sub(1)),
            // INR L        1    Z, S, P, AC       L <- L+1
            0x2c => self.register.l = self.inr_add(self.register.l),
            // DCR L        1    Z, S, P, AC       L <- L-1
            0x2d => self.register.l = self.dcr_sub(self.register.l),
            // MVI L, D8    2                      L <- byte 2
            0x2e => self.register.l = self.get_next_byte(),
            // CMA          1                      A <- !A
            0x2f => self.register.a = !self.register.a,
            // SIM          1                      special    TODO Space dont need
            0x30 => { /* Nothing */ }
            // LXI SP, D16  3                      SP.hi <- byte 3, SP.lo <- byte 2
            0x31 => {
                let word = self.get_next_word();
                self.register.sp = word;
            }
            // STA adr      3                      (adr) <- A
            0x32 => {
                let addr = self.get_next_word();
                self.addring.set_mem(addr, self.register.a)
            }
            // INX SP       1                      SP = SP + 1
            0x33 => self.register.sp = self.register.sp.wrapping_add(1),
            // INR M        1    Z, S, P, AC       (HL) <- (HL)+1
            0x34 => {
                let addr = self.register.get_hl();
                let new_value = self.inr_add(self.addring.get_mem(addr));
                self.addring.set_mem(addr, new_value);
            }
            // DCR M        1    Z, S, P, AC       (HL) <- (HL)-1
            0x35 => {
                let addr = self.register.get_hl();
                let new_value = self.dcr_sub(self.addring.get_mem(addr));
                self.addring.set_mem(addr, new_value);
            }
            // MVI M,D8     2                      (HL) <- byte 2
            0x36 => {
                let byte = self.get_next_byte();
                self.addring.set_mem(self.register.get_hl(), byte);
            }
            // STC          1    CY                CY = 1
            0x37 => self.register.flag_cy = true,
            // -
            0x38 => { /* Nothing */ }
            // DAD SP       1    CY                 HL = HL + SP
            0x39 => self.dad_add(self.register.sp),
            // LDA adr      3                       A <- (adr)
            0x3a => {
                let addr = self.get_next_word();
                self.register.a = self.addring.get_mem(addr)
            }
            // DCX SP       1                       SP = SP-1
            0x3b => self.register.sp = self.register.sp.wrapping_sub(1),
            // INR A        1    Z, S, P, AC        A <- A+1
            0x3c => self.register.a = self.inr_add(self.register.a),
            // DCR A        1    Z, S, P, AC        A <- A-1
            0x3d => self.register.a = self.dcr_sub(self.register.a),
            // MVI A,D8     2                       A <- byte 2
            0x3e => self.register.a = self.get_next_byte(),
            // CMC          1    CY                 CY=!CY
            0x3f => self.register.flag_cy = !self.register.flag_cy,
            // MOV B,B      1                       B <- B
            0x40 => { /* Nothing */ }
            // MOV B,C      1                       B <- C
            0x41 => self.register.b = self.register.c,
            // MOV B,D      1                       B <- D
            0x42 => self.register.b = self.register.d,
            // MOV B,E      1                       B <- E
            0x43 => self.register.b = self.register.e,
            // MOV B,H      1                       B <- H
            0x44 => self.register.b = self.register.h,
            // MOV B,L      1                       B <- L
            0x45 => self.register.b = self.register.l,
            // MOV B,M      1                       B <- (HL)
            0x46 => self.register.b = self.addring.get_mem(self.register.get_hl()),
            // MOV B,A      1                       B <- A
            0x47 => self.register.b = self.register.a,
            // MOV C,B      1                       C <- B
            0x48 => self.register.c = self.register.b,
            // MOV C,C      1                       C <- C
            0x49 => { /* Nothing */ }
            // MOV C,D      1                       C <- D
            0x4a => self.register.c = self.register.d,
            // MOV C,E      1                       C <- E
            0x4b => self.register.c = self.register.e,
            // MOV C,H      1                       C <- H
            0x4c => self.register.c = self.register.h,
            // MOV C,L      1                       C <- L
            0x4d => self.register.c = self.register.l,
            // MOV C,M      1                       C <- (HL)
            0x4e => self.register.c = self.addring.get_mem(self.register.get_hl()),
            // MOV C,A      1                       C <- A
            0x4f => self.register.c = self.register.a,
            // MOV D,B      1                       D <- B
            0x50 => self.register.d = self.register.b,
            // MOV D,C      1                       D <- C
            0x51 => self.register.d = self.register.c,
            // MOV D,D      1                       D <- D
            0x52 => { /* Nothing */ }
            // MOV D,E      1                       D <- E
            0x53 => self.register.d = self.register.e,
            // MOV D,H      1                       D <- H
            0x54 => self.register.d = self.register.h,
            // MOV D,L      1                       D <- L
            0x55 => self.register.d = self.register.l,
            // MOV D,M      1                       D <- (HL)
            0x56 => self.register.d = self.addring.get_mem(self.register.get_hl()),
            // MOV D,A      1                       D <- A
            0x57 => self.register.d = self.register.a,
            // MOV E,B      1                       E <- B
            0x58 => self.register.e = self.register.b,
            // MOV E,C      1                       E <- C
            0x59 => self.register.e = self.register.c,
            // MOV E,D      1                       E <- D
            0x5a => self.register.e = self.register.d,
            // MOV E,E      1                       E <- E
            0x5b => { /* Nothing */ }
            // MOV E,H      1                       E <- H
            0x5c => self.register.e = self.register.h,
            // MOV E,L      1                       E <- L
            0x5d => self.register.e = self.register.l,
            // MOV E,M      1                       E <- (HL)
            0x5e => self.register.e = self.addring.get_mem(self.register.get_hl()),
            // MOV E,A      1                       E <- A
            0x5f => self.register.e = self.register.a,
            // MOV H,B      1                       H <- B
            0x60 => self.register.h = self.register.b,
            // MOV H,C      1                       H <- C
            0x61 => self.register.h = self.register.c,
            // MOV H,D      1                       H <- D
            0x62 => self.register.h = self.register.d,
            // MOV H,E      1                       H <- E
            0x63 => self.register.h = self.register.e,
            // MOV H,H      1                       H <- H
            0x64 => { /* Nothing */ }
            // MOV H,L      1                       H <- L
            0x65 => self.register.h = self.register.l,
            // MOV H,M      1                       H <- (HL)
            0x66 => self.register.h = self.addring.get_mem(self.register.get_hl()),
            // MOV H,A      1                       H <- A
            0x67 => self.register.h = self.register.a,
            // MOV L,B      1                       L <- B
            0x68 => self.register.l = self.register.b,
            // MOV L,C      1                       L <- C
            0x69 => self.register.l = self.register.c,
            // MOV L,D      1                       L <- D
            0x6a => self.register.l = self.register.d,
            // MOV L,E      1                       L <- E
            0x6b => self.register.l = self.register.e,
            // MOV L,H      1                       L <- H
            0x6c => self.register.l = self.register.h,
            // MOV L,L      1                       L <- L
            0x6d => { /* Nothing */ }
            // MOV L,M      1                       L <- (HL)
            0x6e => self.register.l = self.addring.get_mem(self.register.get_hl()),
            // MOV L,A      1                       L <- A
            0x6f => self.register.l = self.register.a,
            // MOV M,B      1                       (HL) <- B
            0x70 => self
                .addring
                .set_mem(self.register.get_hl(), self.register.b),
            // MOV M,C      1                       (HL) <- C
            0x71 => self
                .addring
                .set_mem(self.register.get_hl(), self.register.c),
            // MOV M,D      1                       (HL) <- D
            0x72 => self
                .addring
                .set_mem(self.register.get_hl(), self.register.d),
            // MOV M,E      1                       (HL) <- E
            0x73 => self
                .addring
                .set_mem(self.register.get_hl(), self.register.e),
            // MOV M,H      1                       (HL) <- H
            0x74 => self
                .addring
                .set_mem(self.register.get_hl(), self.register.h),
            // MOV M,L      1                       (HL) <- L
            0x75 => self
                .addring
                .set_mem(self.register.get_hl(), self.register.l),
            // HLT          1                       special   HALT INSTRUCTION
            0x76 => ::std::process::exit(0),
            // MOV M,A      1                       (HL) <- A
            0x77 => self
                .addring
                .set_mem(self.register.get_hl(), self.register.a),
            // MOV A,B      1                       A <- B
            0x78 => self.register.a = self.register.b,
            // MOV A,C      1                       A <- C
            0x79 => self.register.a = self.register.c,
            // MOV A,D      1                       A <- D
            0x7a => self.register.a = self.register.d,
            // MOV A,E      1                       A <- E
            0x7b => self.register.a = self.register.e,
            // MOV A,H      1                       A <- H
            0x7c => self.register.a = self.register.h,
            // MOV A,L      1                       A <- L
            0x7d => self.register.a = self.register.l,
            // MOV A,M      1                       A <- (HL)
            0x7e => self.register.a = self.addring.get_mem(self.register.get_hl()),
            // MOV A,A      1                       A <- A
            0x7f => { /* Nothing */ }
            // ADD B        1    Z, S, P, CY, AC    A <- A + B
            0x80 => self.add(self.register.b),
            // ADD C        1    Z, S, P, CY, AC    A <- A + C
            0x81 => self.add(self.register.c),
            // ADD D        1    Z, S, P, CY, AC    A <- A + D
            0x82 => self.add(self.register.d),
            // ADD E        1    Z, S, P, CY, AC    A <- A + E
            0x83 => self.add(self.register.e),
            // ADD H        1    Z, S, P, CY, AC    A <- A + H
            0x84 => self.add(self.register.h),
            // ADD L        1    Z, S, P, CY, AC    A <- A + L
            0x85 => self.add(self.register.l),
            // ADD M        1    Z, S, P, CY, AC    A <- A + (HL)
            0x86 => self.add(self.addring.get_mem(self.register.get_hl())),
            // ADD A        1    Z, S, P, CY, AC    A <- A + A
            0x87 => self.add(self.register.a),
            // ADC B        1    Z, S, P, CY, AC    A <- A + B + CY
            0x88 => self.adc(self.register.b),
            // ADC C        1    Z, S, P, CY, AC    A <- A + C + CY
            0x89 => self.adc(self.register.c),
            // ADC D        1    Z, S, P, CY, AC    A <- A + D + CY
            0x8a => self.adc(self.register.d),
            // ADC E        1    Z, S, P, CY, AC    A <- A + E + CY
            0x8b => self.adc(self.register.e),
            // ADC H        1    Z, S, P, CY, AC    A <- A + H + CY
            0x8c => self.adc(self.register.h),
            // ADC L        1    Z, S, P, CY, AC    A <- A + L + CY
            0x8d => self.adc(self.register.l),
            // ADC M        1    Z, S, P, CY, AC    A <- A + (HL) + CY
            0x8e => self.adc(self.addring.get_mem(self.register.get_hl())),
            // ADC A        1    Z, S, P, CY, AC    A <- A + A + CY
            0x8f => self.adc(self.register.a),
            // SUB B        1    Z, S, P, CY, AC    A <- A - B
            0x90 => self.sub(self.register.b),
            // SUB C        1    Z, S, P, CY, AC    A <- A - C
            0x91 => self.sub(self.register.c),
            // SUB D        1    Z, S, P, CY, AC    A <- A + D
            0x92 => self.sub(self.register.d),
            // SUB E        1    Z, S, P, CY, AC    A <- A - E
            0x93 => self.sub(self.register.e),
            // SUB H        1    Z, S, P, CY, AC    A <- A + H
            0x94 => self.sub(self.register.h),
            // SUB L        1    Z, S, P, CY, AC    A <- A - L
            0x95 => self.sub(self.register.l),
            // SUB M        1    Z, S, P, CY, AC    A <- A + (HL)
            0x96 => self.sub(self.addring.get_mem(self.register.get_hl())),
            // SUB A        1    Z, S, P, CY, AC    A <- A - A
            0x97 => self.sub(self.register.a),
            // SBB B        1    Z, S, P, CY, AC    A <- A - B - CY
            0x98 => self.sbb(self.register.b),
            // SBB C        1    Z, S, P, CY, AC    A <- A - C - CY
            0x99 => self.sbb(self.register.c),
            // SBB D        1    Z, S, P, CY, AC    A <- A - D - CY
            0x9a => self.sbb(self.register.d),
            // SBB E        1    Z, S, P, CY, AC    A <- A - E - CY
            0x9b => self.sbb(self.register.e),
            // SBB H        1    Z, S, P, CY, AC    A <- A - H - CY
            0x9c => self.sbb(self.register.h),
            // SBB L        1    Z, S, P, CY, AC    A <- A - L - CY
            0x9d => self.sbb(self.register.l),
            // SBB M        1    Z, S, P, CY, AC    A <- A - (HL) - CY
            0x9e => self.sbb(self.addring.get_mem(self.register.get_hl())),
            // SBB A        1    Z, S, P, CY, AC    A <- A - A - CY
            0x9f => self.sbb(self.register.a),
            // ANA B        1    Z, S, P, CY, AC    A <- A & B
            0xa0 => self.ana(self.register.b),
            // ANA C        1    Z, S, P, CY, AC    A <- A & C
            0xa1 => self.ana(self.register.c),
            // ANA D        1    Z, S, P, CY, AC    A <- A & D
            0xa2 => self.ana(self.register.d),
            // ANA E        1    Z, S, P, CY, AC    A <- A & E
            0xa3 => self.ana(self.register.e),
            // ANA H        1    Z, S, P, CY, AC    A <- A & H
            0xa4 => self.ana(self.register.h),
            // ANA L        1    Z, S, P, CY, AC    A <- A & L
            0xa5 => self.ana(self.register.l),
            // ANA M        1    Z, S, P, CY, AC    A <- A & (HL)
            0xa6 => self.ana(self.addring.get_mem(self.register.get_hl())),
            // ANA A        1    Z, S, P, CY, AC    A <- A & A
            0xa7 => self.ana(self.register.a),
            // XRA B        1    Z, S, P, CY, AC    A <- A ^ B
            0xa8 => self.xra(self.register.b),
            // XRA C        1    Z, S, P, CY, AC    A <- A ^ C
            0xa9 => self.xra(self.register.c),
            // XRA D        1    Z, S, P, CY, AC    A <- A ^ D
            0xaa => self.xra(self.register.d),
            // XRA E        1    Z, S, P, CY, AC    A <- A ^ E
            0xab => self.xra(self.register.e),
            // XRA H        1    Z, S, P, CY, AC    A <- A ^ H
            0xac => self.xra(self.register.h),
            // XRA L        1    Z, S, P, CY, AC    A <- A ^ L
            0xad => self.xra(self.register.l),
            // XRA M        1    Z, S, P, CY, AC    A <- A ^ (HL)
            0xae => self.xra(self.addring.get_mem(self.register.get_hl())),
            // XRA A        1    Z, S, P, CY, AC    A <- A ^ A
            0xaf => self.xra(self.register.a),
            // ORA B        1    Z, S, P, CY, AC    A <- A | B
            0xb0 => self.ora(self.register.b),
            // ORA C        1    Z, S, P, CY, AC    A <- A | C
            0xb1 => self.ora(self.register.c),
            // ORA D        1    Z, S, P, CY, AC    A <- A | D
            0xb2 => self.ora(self.register.d),
            // ORA E        1    Z, S, P, CY, AC    A <- A | E
            0xb3 => self.ora(self.register.e),
            // ORA H        1    Z, S, P, CY, AC    A <- A | H
            0xb4 => self.ora(self.register.h),
            // ORA L        1    Z, S, P, CY, AC    A <- A | L
            0xb5 => self.ora(self.register.l),
            // ORA M        1    Z, S, P, CY, AC    A <- A | (HL)
            0xb6 => self.ora(self.addring.get_mem(self.register.get_hl())),
            // ORA A        1    Z, S, P, CY, AC    A <- A | A
            0xb7 => self.ora(self.register.a),
            // CMP B        1    Z, S, P, CY, AC    A - B
            0xb8 => self.cmp(self.register.b),
            // CMP C        1    Z, S, P, CY, AC    A - C
            0xb9 => self.cmp(self.register.c),
            // CMP D        1    Z, S, P, CY, AC    A - D
            0xba => self.cmp(self.register.d),
            // CMP E        1    Z, S, P, CY, AC    A - E
            0xbb => self.cmp(self.register.e),
            // CMP H        1    Z, S, P, CY, AC    A - H
            0xbc => self.cmp(self.register.h),
            // CMP L        1    Z, S, P, CY, AC    A - L
            0xbd => self.cmp(self.register.l),
            // CMP M        1    Z, S, P, CY, AC    A - (HL)
            0xbe => self.cmp(self.addring.get_mem(self.register.get_hl())),
            // CMP A        1    Z, S, P, CY, AC    A - A
            0xbf => self.cmp(self.register.a),
            // RNZ          1                       if NZ, RET
            0xc0 => {
                if !self.register.flag_z {
                    self.register.pc = self.stack_pop();
                    ex_cycle = true;
                }
            }
            // POP B        1                       C <- (sp); B <- (sp+1); sp <- sp+2
            0xc1 => {
                let value = self.stack_pop();
                self.register.set_bc(value);
            }
            // JNZ adr      3                       if NZ, PC <- adr
            0xc2 => self.condition_jmp(!self.register.flag_z),
            // JMP adr      3                       PC <= adr
            0xc3 => self.jmp(),
            // CNZ adr      3                       if NZ, CALL adr
            0xc4 => ex_cycle = self.condition_call(!self.register.flag_z),
            // PUSH B       1                       (sp-2)<-C; (sp-1)<-B; sp <- sp - 2
            0xc5 => self.stack_add(self.register.get_bc()),
            // ADI D8       2    Z, S, P, CY, AC    A <- A + byte
            0xc6 => {
                let data = self.get_next_byte();
                self.add(data);
            }
            // RST 0        1                       CALL $0
            0xc7 => self.rst(op_code),
            // RZ           1                       if Z, RET
            0xc8 => {
                if self.register.flag_z {
                    self.register.pc = self.stack_pop();
                    ex_cycle = true;
                }
            }
            // RET          1                       PC.lo <- (sp); PC.hi<-(sp+1); SP <- SP+2
            0xc9 => self.register.pc = self.stack_pop(),
            // JZ adr       3                       if Z, PC <- adr
            0xca => self.condition_jmp(self.register.flag_z),
            // -
            0xcb => {
                eprintln!("未实现 {:#04X}", op_code);
                // TODO
            }
            // CZ adr       3                       if Z, CALL adr
            0xcc => ex_cycle = self.condition_call(self.register.flag_z),
            // CALL adr     3                       (SP-1)<-PC.hi;(SP-2)<-PC.lo;SP<-SP+2;PC=adr
            // push stack ,then JMP
            0xcd => self.call(),
            // ACI D8       2    Z, S, P, CY, AC    A <- A + data + CY
            0xce => {
                let data = self.get_next_byte();
                self.adc(data);
            }
            // RST 1        1                       CALL $8
            0xcf => self.rst(op_code),
            // RNC          1                       if NCY, RET
            0xd0 => {
                if !self.register.flag_cy {
                    self.register.pc = self.stack_pop();
                    ex_cycle = true;
                }
            }
            // POP D        1                       E <- (sp); D <- (sp+1); sp <- sp+2
            0xd1 => {
                let value = self.stack_pop();
                self.register.set_de(value);
            }
            // JNC adr      3                       if NCY, PC<-adr
            0xd2 => self.condition_jmp(!self.register.flag_cy),
            // OUT D8       2                       special
            0xd3 => {
                //eprintln!("未完整实现 {:#04X}", op_code);
                let _byte = self.get_next_byte();
            }
            // CNC adr      3                       if NCY, CALL adr
            0xd4 => ex_cycle = self.condition_call(!self.register.flag_cy),
            // PUSH D       1                       (sp-2)<-E; (sp-1)<-D; sp <- sp - 2
            0xd5 => self.stack_add(self.register.get_de()),
            // SUI D8       2    Z, S, P, CY, AC    A <- A - data
            0xd6 => {
                let data = self.get_next_byte();
                self.sub(data);
            }
            // RST 2        1                       CALL $10
            0xd7 => self.rst(op_code),
            // RC           1                       if CY, RET
            0xd8 => {
                if self.register.flag_cy {
                    self.register.pc = self.stack_pop();
                    ex_cycle = true;
                }
            }
            // - 0xC9
            0xd9 => self.register.pc = self.stack_pop(),
            // JC adr       3                       if CY, PC<-adr
            0xda => self.condition_jmp(self.register.flag_cy),
            // IN D8        2                       special
            0xdb => {
                let byte = self.get_next_byte();
                let ref_mut = &mut self.io;
                ref_mut.borrow_mut().input(&mut self.register, byte);
            }
            // CC adr       3                       if CY, CALL adr
            0xdc => ex_cycle = self.condition_call(self.register.flag_cy),
            // -
            0xdd => self.call(),
            // SBI D8       2    Z, S, P, CY, AC    A <- A - data - CY
            0xde => {
                let data = self.get_next_byte();
                self.sbb(data);
            }
            // RST 3        1                       CALL $18
            0xdf => self.rst(op_code),
            // RPO          1                       if PO, RET
            0xe0 => {
                if !self.register.flag_p {
                    self.register.pc = self.stack_pop();
                    ex_cycle = true;
                }
            }
            // POP H        1                       L <- (sp); H <- (sp+1); sp <- sp+2
            0xe1 => {
                let value = self.stack_pop();
                self.register.set_hl(value);
            }
            // JPO adr      3                       if PO, PC <- adr
            0xe2 => self.condition_jmp(!self.register.flag_p),
            // XTHL         1                       L <-> (SP); H <-> (SP+1)
            0xe3 => {
                let addr = self.addring.get_word(self.register.sp);
                let hl = self.register.get_hl();
                self.register.set_hl(addr);
                self.addring.set_word(self.register.sp, hl);
            }
            // CPO adr      3                       if PO, CALL adr
            0xe4 => ex_cycle = self.condition_call(!self.register.flag_p),
            // PUSH H       1                       (sp-2)<-L; (sp-1)<-H; sp <- sp - 2
            0xe5 => self.stack_add(self.register.get_hl()),
            // ANI D8       2    Z, S, P, CY, AC    A <- A & data
            0xe6 => {
                let data = self.get_next_byte();
                self.ana(data);
            }
            // RST 4        1                       CALL $20
            0xe7 => self.rst(op_code),
            // RPE          1                       if PE, RET
            0xe8 => {
                if self.register.flag_p {
                    self.register.pc = self.stack_pop();
                    ex_cycle = true;
                }
            }
            // PCHL         1                       PC.hi <- H; PC.lo <- L
            0xe9 => self.register.pc = self.register.get_hl(),
            // JPE adr      3                       if PE, PC <- adr
            0xea => self.condition_jmp(self.register.flag_p),
            // XCHG         1                       H <-> D; L <-> E
            0xeb => {
                mem::swap(&mut self.register.h, &mut self.register.d);
                mem::swap(&mut self.register.l, &mut self.register.e);
            }
            // CPE adr      3                       if PE, CALL adr    Parity Even
            0xec => ex_cycle = self.condition_call(self.register.flag_p),
            // -
            0xed => self.call(),
            // XRI D8       2    Z, S, P, CY, AC    A <- A ^ data
            0xee => {
                let data = self.get_next_byte();
                self.xra(data);
            }
            // RST 5        1                       CALL $28
            0xef => self.rst(op_code),
            // RP           1                       if P, RET
            0xf0 => {
                if !self.register.flag_s {
                    self.register.pc = self.stack_pop();
                    ex_cycle = true;
                }
            }
            // POP PSW      1                       flags <- (sp); A <- (sp+1); sp <- sp+2
            0xf1 => {
                let value = self.stack_pop();
                self.register.a = (value >> 8) as u8;
                self.register.set_flags((value & 0x00d5 | 0x0002) as u8);
            }
            // JP adr       3                       if S=0, PC <- adr
            0xf2 => self.condition_jmp(!self.register.flag_s),
            // DI           1                       special
            0xf3 => self.interrupt = false,
            // CP adr       3                       if P, PC <- adr    Call if  Plus
            0xf4 => ex_cycle = self.condition_call(!self.register.flag_s),
            // PUSH PSW     1                       (sp-2)<-flags; (sp-1)<-A; sp <- sp - 2
            0xf5 => {
                self.stack_add(
                    (u16::from(self.register.a) << 8) | u16::from(self.register.get_flags()),
                );
            }
            // ORI D8       2    Z, S, P, CY, AC    A <- A | data
            0xf6 => {
                let data = self.get_next_byte();
                self.ora(data);
            }
            // RST 6        1                       CALL $30
            0xf7 => self.rst(op_code),
            // RM           1                       if M, RET
            0xf8 => {
                if self.register.flag_s {
                    self.register.pc = self.stack_pop();
                    ex_cycle = true;
                }
            }
            // SPHL         1                       SP=HL
            0xf9 => self.register.sp = self.register.get_hl(),
            // JM adr       3                       if M, PC <- adr
            0xfa => self.condition_jmp(self.register.flag_s),
            // EI           1                       special
            0xfb => self.interrupt = true,
            // CM adr       3                       if M, CALL adr   Call If Minus
            0xfc => ex_cycle = self.condition_call(self.register.flag_s),
            // -
            0xfd => self.call(),
            // CPI D8       2    Z, S, P, CY, AC    A - data
            0xfe => {
                let data = self.get_next_byte();
                self.cmp(data);
            }
            // RST 7        1                       CALL $38
            0xff => self.rst(op_code),
            n => println!("unknow opcode 0x{:X}", n),
        };
        if ex_cycle {
            OP_CYCLES[op_code as usize] + 6
        } else {
            OP_CYCLES[op_code as usize]
        }
    }

    pub fn interrupt(&mut self, code: u8) -> bool {
        if self.interrupt {
            self.rst(code);
            self.interrupt = false;
            true
        } else {
            false
        }
    }
}

//  0   1   2   3   4   5   6   7   8   9   a   b   c   d   e   f
const OP_CYCLES: [u8; 256] = [
    04, 10, 07, 05, 05, 05, 07, 04, 04, 10, 07, 05, 05, 05, 07, 04, // 0
    04, 10, 07, 05, 05, 05, 07, 04, 04, 10, 07, 05, 05, 05, 07, 04, // 1
    04, 10, 16, 05, 05, 05, 07, 04, 04, 10, 16, 05, 05, 05, 07, 04, // 2
    04, 10, 13, 05, 10, 10, 10, 04, 04, 10, 13, 05, 05, 05, 07, 04, // 3
    05, 05, 05, 05, 05, 05, 07, 05, 05, 05, 05, 05, 05, 05, 07, 05, // 4
    05, 05, 05, 05, 05, 05, 07, 05, 05, 05, 05, 05, 05, 05, 07, 05, // 5
    05, 05, 05, 05, 05, 05, 07, 05, 05, 05, 05, 05, 05, 05, 07, 05, // 6
    07, 07, 07, 07, 07, 07, 07, 07, 05, 05, 05, 05, 05, 05, 07, 05, // 7
    04, 04, 04, 04, 04, 04, 07, 04, 04, 04, 04, 04, 04, 04, 07, 04, // 8
    04, 04, 04, 04, 04, 04, 07, 04, 04, 04, 04, 04, 04, 04, 07, 04, // 9
    04, 04, 04, 04, 04, 04, 07, 04, 04, 04, 04, 04, 04, 04, 07, 04, // a
    04, 04, 04, 04, 04, 04, 07, 04, 04, 04, 04, 04, 04, 04, 07, 04, // b
    05, 10, 10, 10, 11, 11, 07, 11, 05, 10, 10, 10, 11, 17, 07, 11, // c
    05, 10, 10, 10, 11, 11, 07, 11, 05, 10, 10, 10, 11, 17, 07, 11, // d
    05, 10, 10, 18, 11, 11, 07, 11, 05, 05, 10, 05, 11, 17, 07, 11, // e
    05, 10, 10, 04, 11, 11, 07, 11, 05, 05, 10, 04, 11, 17, 07, 11, // f
];
