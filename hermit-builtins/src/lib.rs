#![no_std]
#![feature(linkage)]

pub mod math;

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
	loop {}
}
