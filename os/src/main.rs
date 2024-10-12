#![no_std]
#![no_main]
#![feature(panic_info_message)]

use core::{
    arch::global_asm,
    fmt::{self, Write},
};

use sbi::shutdown;

mod console;
mod lang_items;
mod sbi;

#[path = "boards/qemu.rs"]
mod board;

global_asm!(include_str!("entry.asm"));

fn clear_bss() {
    extern "C" {
        fn sbss();
        fn ebss();
    }

    (sbss as usize..ebss as usize).for_each(|a| unsafe {
        (a as *mut u8).write_volatile(0);
    });
}

#[no_mangle]
extern "C" fn rust_main() {
    println!("hello world!");
    shutdown();
}
