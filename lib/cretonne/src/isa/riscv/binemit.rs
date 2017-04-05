//! Emitting binary RISC-V machine code.

use binemit::{CodeSink, Reloc, bad_encoding};
use ir::{Function, Inst, InstructionData};
use isa::RegUnit;

include!(concat!(env!("OUT_DIR"), "/binemit-riscv.rs"));

/// RISC-V relocation kinds.
pub enum RelocKind {
    /// A conditional (SB-type) branch to an EBB.
    Branch,
}

pub static RELOC_NAMES: [&'static str; 1] = ["Branch"];

impl Into<Reloc> for RelocKind {
    fn into(self) -> Reloc {
        Reloc(self as u16)
    }
}

/// R-type instructions.
///
///   31     24  19  14     11 6
///   funct7 rs2 rs1 funct3 rd opcode
///       25  20  15     12  7      0
///
/// Encoding bits: `opcode[6:2] | (funct3 << 5) | (funct7 << 8)`.
fn put_r<CS: CodeSink + ?Sized>(bits: u16,
                                rs1: RegUnit,
                                rs2: RegUnit,
                                rd: RegUnit,
                                sink: &mut CS) {
    let bits = bits as u32;
    let opcode5 = bits & 0x1f;
    let funct3 = (bits >> 5) & 0x7;
    let funct7 = (bits >> 8) & 0x7f;
    let rs1 = rs1 as u32 & 0x1f;
    let rs2 = rs2 as u32 & 0x1f;
    let rd = rd as u32 & 0x1f;

    // 0-6: opcode
    let mut i = 0x3;
    i |= opcode5 << 2;
    i |= rd << 7;
    i |= funct3 << 12;
    i |= rs1 << 15;
    i |= rs2 << 20;
    i |= funct7 << 25;

    sink.put4(i);
}

/// R-type instructions with a shift amount instead of rs2.
///
///   31     25    19  14     11 6
///   funct7 shamt rs1 funct3 rd opcode
///       25    20  15     12  7      0
///
/// Both funct7 and shamt contribute to bit 25. In RV64, shamt uses it for shifts > 31.
///
/// Encoding bits: `opcode[6:2] | (funct3 << 5) | (funct7 << 8)`.
fn put_rshamt<CS: CodeSink + ?Sized>(bits: u16,
                                     rs1: RegUnit,
                                     shamt: i64,
                                     rd: RegUnit,
                                     sink: &mut CS) {
    let bits = bits as u32;
    let opcode5 = bits & 0x1f;
    let funct3 = (bits >> 5) & 0x7;
    let funct7 = (bits >> 8) & 0x7f;
    let rs1 = rs1 as u32 & 0x1f;
    let shamt = shamt as u32 & 0x3f;
    let rd = rd as u32 & 0x1f;

    // 0-6: opcode
    let mut i = 0x3;
    i |= opcode5 << 2;
    i |= rd << 7;
    i |= funct3 << 12;
    i |= rs1 << 15;
    i |= shamt << 20;
    i |= funct7 << 25;

    sink.put4(i);
}

fn recipe_r<CS: CodeSink + ?Sized>(func: &Function, inst: Inst, sink: &mut CS) {
    if let InstructionData::Binary { args, .. } = func.dfg[inst] {
        put_r(func.encodings[inst].bits(),
              func.locations[args[0]].unwrap_reg(),
              func.locations[args[1]].unwrap_reg(),
              func.locations[func.dfg.first_result(inst)].unwrap_reg(),
              sink);
    } else {
        panic!("Expected Binary format: {:?}", func.dfg[inst]);
    }
}

fn recipe_ricmp<CS: CodeSink + ?Sized>(func: &Function, inst: Inst, sink: &mut CS) {
    if let InstructionData::IntCompare { args, .. } = func.dfg[inst] {
        put_r(func.encodings[inst].bits(),
              func.locations[args[0]].unwrap_reg(),
              func.locations[args[1]].unwrap_reg(),
              func.locations[func.dfg.first_result(inst)].unwrap_reg(),
              sink);
    } else {
        panic!("Expected IntCompare format: {:?}", func.dfg[inst]);
    }
}

fn recipe_rshamt<CS: CodeSink + ?Sized>(func: &Function, inst: Inst, sink: &mut CS) {
    if let InstructionData::BinaryImm { arg, imm, .. } = func.dfg[inst] {
        put_rshamt(func.encodings[inst].bits(),
                   func.locations[arg].unwrap_reg(),
                   imm.into(),
                   func.locations[func.dfg.first_result(inst)].unwrap_reg(),
                   sink);
    } else {
        panic!("Expected BinaryImm format: {:?}", func.dfg[inst]);
    }
}

/// I-type instructions.
///
///   31  19  14     11 6
///   imm rs1 funct3 rd opcode
///    20  15     12  7      0
///
/// Encoding bits: `opcode[6:2] | (funct3 << 5)`
fn put_i<CS: CodeSink + ?Sized>(bits: u16, rs1: RegUnit, imm: i64, rd: RegUnit, sink: &mut CS) {
    let bits = bits as u32;
    let opcode5 = bits & 0x1f;
    let funct3 = (bits >> 5) & 0x7;
    let rs1 = rs1 as u32 & 0x1f;
    let rd = rd as u32 & 0x1f;

    // 0-6: opcode
    let mut i = 0x3;
    i |= opcode5 << 2;
    i |= rd << 7;
    i |= funct3 << 12;
    i |= rs1 << 15;
    i |= (imm << 20) as u32;

    sink.put4(i);
}

fn recipe_i<CS: CodeSink + ?Sized>(func: &Function, inst: Inst, sink: &mut CS) {
    if let InstructionData::BinaryImm { arg, imm, .. } = func.dfg[inst] {
        put_i(func.encodings[inst].bits(),
              func.locations[arg].unwrap_reg(),
              imm.into(),
              func.locations[func.dfg.first_result(inst)].unwrap_reg(),
              sink);
    } else {
        panic!("Expected BinaryImm format: {:?}", func.dfg[inst]);
    }
}

fn recipe_iicmp<CS: CodeSink + ?Sized>(func: &Function, inst: Inst, sink: &mut CS) {
    if let InstructionData::IntCompareImm { arg, imm, .. } = func.dfg[inst] {
        put_i(func.encodings[inst].bits(),
              func.locations[arg].unwrap_reg(),
              imm.into(),
              func.locations[func.dfg.first_result(inst)].unwrap_reg(),
              sink);
    } else {
        panic!("Expected IntCompareImm format: {:?}", func.dfg[inst]);
    }
}

fn recipe_iret<CS: CodeSink + ?Sized>(_func: &Function, _inst: Inst, _sink: &mut CS) {
    unimplemented!()
}

/// U-type instructions.
///
///   31  11 6
///   imm rd opcode
///    12  7      0
///
/// Encoding bits: `opcode[6:2] | (funct3 << 5)`
fn put_u<CS: CodeSink + ?Sized>(bits: u16, imm: i64, rd: RegUnit, sink: &mut CS) {
    let bits = bits as u32;
    let opcode5 = bits & 0x1f;
    let rd = rd as u32 & 0x1f;

    // 0-6: opcode
    let mut i = 0x3;
    i |= opcode5 << 2;
    i |= rd << 7;
    i |= imm as u32 & 0xfffff000;

    sink.put4(i);
}

fn recipe_u<CS: CodeSink + ?Sized>(func: &Function, inst: Inst, sink: &mut CS) {
    if let InstructionData::UnaryImm { imm, .. } = func.dfg[inst] {
        put_u(func.encodings[inst].bits(),
              imm.into(),
              func.locations[func.dfg.first_result(inst)].unwrap_reg(),
              sink);
    } else {
        panic!("Expected UnaryImm format: {:?}", func.dfg[inst]);
    }
}

/// SB-type branch instructions.
///
///   31  24  19  14     11  6
///   imm rs2 rs1 funct3 imm opcode
///    25  20  15     12   7      0
///
/// The imm bits are not encoded by this function. They encode the relative distance to the
/// destination block, handled by a relocation.
///
/// Encoding bits: `opcode[6:2] | (funct3 << 5)`
fn put_sb<CS: CodeSink + ?Sized>(bits: u16, rs1: RegUnit, rs2: RegUnit, sink: &mut CS) {
    let bits = bits as u32;
    let opcode5 = bits & 0x1f;
    let funct3 = (bits >> 5) & 0x7;
    let rs1 = rs1 as u32 & 0x1f;
    let rs2 = rs2 as u32 & 0x1f;

    // 0-6: opcode
    let mut i = 0x3;
    i |= opcode5 << 2;
    i |= funct3 << 12;
    i |= rs1 << 15;
    i |= rs2 << 20;

    sink.put4(i);
}

fn recipe_sb<CS: CodeSink + ?Sized>(func: &Function, inst: Inst, sink: &mut CS) {
    if let InstructionData::BranchIcmp {
               destination,
               ref args,
               ..
           } = func.dfg[inst] {
        let args = &args.as_slice(&func.dfg.value_lists)[0..2];
        sink.reloc_ebb(RelocKind::Branch.into(), destination);
        put_sb(func.encodings[inst].bits(),
               func.locations[args[0]].unwrap_reg(),
               func.locations[args[1]].unwrap_reg(),
               sink);
    } else {
        panic!("Expected BranchIcmp format: {:?}", func.dfg[inst]);
    }
}

fn recipe_sbzero<CS: CodeSink + ?Sized>(func: &Function, inst: Inst, sink: &mut CS) {
    if let InstructionData::Branch {
               destination,
               ref args,
               ..
           } = func.dfg[inst] {
        let args = &args.as_slice(&func.dfg.value_lists)[0..1];
        sink.reloc_ebb(RelocKind::Branch.into(), destination);
        put_sb(func.encodings[inst].bits(),
               func.locations[args[0]].unwrap_reg(),
               0,
               sink);
    } else {
        panic!("Expected Branch format: {:?}", func.dfg[inst]);
    }
}
