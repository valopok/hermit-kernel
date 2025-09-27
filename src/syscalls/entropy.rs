#[cfg(not(feature = "newlib"))]
use core::mem::size_of;
use core::slice;

use hermit_sync::TicketMutex;

use crate::arch;
use crate::entropy::{self, Flags};
use crate::errno::EINVAL;

static PARK_MILLER_LEHMER_SEED: TicketMutex<u32> = TicketMutex::new(0);
const RAND_MAX: u64 = 0x7fff_ffff;

fn generate_park_miller_lehmer_random_number() -> u32 {
	let mut seed = PARK_MILLER_LEHMER_SEED.lock();
	let random = ((u64::from(*seed) * 48271) % RAND_MAX) as u32;
	*seed = random;
	random
}

unsafe fn read_entropy(buf: *mut u8, len: usize, flags: u32) -> isize {
	let Some(flags) = Flags::from_bits(flags) else {
		return -EINVAL as isize;
	};

	let buf = unsafe {
		// Cap the number of bytes to be read at a time to isize::MAX to uphold
		// the safety guarantees of `from_raw_parts`.
		let len = usize::min(len, isize::MAX as usize);
		buf.write_bytes(0, len);
		slice::from_raw_parts_mut(buf, len)
	};

	let ret = entropy::read(buf, flags);
	if ret < 0 {
		warn!("Unable to read entropy! Fallback to a naive implementation!");
		for i in &mut *buf {
			*i = (generate_park_miller_lehmer_random_number() & 0xff)
				.try_into()
				.unwrap();
		}
		buf.len().try_into().unwrap()
	} else {
		ret
	}
}

/// Fill `len` bytes in `buf` with cryptographically secure random data.
///
/// Returns either the number of bytes written to buf (a positive value) or
/// * `-EINVAL` if `flags` contains unknown flags.
/// * `-ENOSYS` if the system does not support random data generation.
#[hermit_macro::system]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sys_read_entropy(buf: *mut u8, len: usize, flags: u32) -> isize {
	unsafe { read_entropy(buf, len, flags) }
}

/// Create a cryptographicly secure 32bit random number with the support of
/// the underlying hardware. If the required hardware isn't available,
/// the function returns `-1`.
#[cfg(not(feature = "newlib"))]
#[hermit_macro::system]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sys_secure_rand32(value: *mut u32) -> i32 {
	let mut buf = value.cast();
	let mut len = size_of::<u32>();
	while len != 0 {
		let res = unsafe { read_entropy(buf, len, 0) };
		if res < 0 {
			return -1;
		}

		buf = unsafe { buf.add(res as usize) };
		len -= res as usize;
	}

	0
}

/// Create a cryptographicly secure 64bit random number with the support of
/// the underlying hardware. If the required hardware isn't available,
/// the function returns -1.
#[cfg(not(feature = "newlib"))]
#[hermit_macro::system]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sys_secure_rand64(value: *mut u64) -> i32 {
	let mut buf = value.cast();
	let mut len = size_of::<u64>();
	while len != 0 {
		let res = unsafe { read_entropy(buf, len, 0) };
		if res < 0 {
			return -1;
		}

		buf = unsafe { buf.add(res as usize) };
		len -= res as usize;
	}

	0
}

/// The function computes a sequence of pseudo-random integers
/// in the range of 0 to RAND_MAX
#[hermit_macro::system]
#[unsafe(no_mangle)]
pub extern "C" fn sys_rand() -> u32 {
	generate_park_miller_lehmer_random_number()
}

/// The function sets its argument as the seed for a new sequence
/// of pseudo-random numbers to be returned by rand()
#[hermit_macro::system]
#[unsafe(no_mangle)]
pub extern "C" fn sys_srand(seed: u32) {
	*(PARK_MILLER_LEHMER_SEED.lock()) = seed;
}

pub(crate) fn init_entropy() {
	let seed: u32 = arch::processor::get_timestamp() as u32;

	*PARK_MILLER_LEHMER_SEED.lock() = seed;
}
