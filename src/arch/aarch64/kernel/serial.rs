use alloc::collections::vec_deque::VecDeque;
use core::ptr::NonNull;

#[cfg(all(feature = "pci", feature = "console"))]
use crate::drivers::pci::get_console_driver;
#[cfg(all(not(feature = "pci"), feature = "console"))]
use crate::kernel::mmio::get_console_driver;
use crate::syscalls::interfaces::serial_buf_hypercall;

enum SerialInner {
	None,
	Uart(u32),
	Uhyve,
	#[cfg(feature = "console")]
	Virtio,
}

impl UartDevice {
	pub fn new() -> Self {
		let base = crate::env::boot_info()
			.hardware_info
			.serial_port_base
			.map(|uartport| uartport.get())
			.unwrap();

		let uart_pointer =
			unsafe { UniqueMmioPointer::new(NonNull::new_unchecked(base as *mut _)) };

		let mut uart = Uart::new(uart_pointer);

		let line_config = LineConfig {
			data_bits: DataBits::Bits8,
			parity: Parity::None,
			stop_bits: StopBits::One,
		};
		uart.enable(line_config, 115_200, 16_000_000).unwrap();

		uart.set_interrupt_masks(Interrupts::RXI | Interrupts::RTI);
		uart.clear_interrupts(Interrupts::all());

		Self {
			uart,
			buffer: VecDeque::new(),
		}
	}
}

impl SerialPort {
	pub fn new(port_address: Option<u64>) -> Self {
		if crate::env::is_uhyve() {
			Self {
				inner: SerialInner::Uhyve,
			}
		} else if let Some(port_address) = port_address {
			Self {
				inner: SerialInner::Uart(port_address.try_into().unwrap()),
			}
		} else {
			Self {
				inner: SerialInner::None,
			}

			Ok(min)
		}
	}

	#[cfg(feature = "console")]
	pub fn switch_to_virtio_console(&mut self) {
		self.inner = SerialInner::Virtio;
	}

	pub fn write_buf(&mut self, buf: &[u8]) {
		match &mut self.inner {
			SerialInner::None => {
				// No serial port configured, do nothing.
			}
			SerialInner::Uhyve => {
				serial_buf_hypercall(buf);
			}
			SerialInner::Uart(port_address) => {
				let port = core::ptr::with_exposed_provenance_mut::<u8>(*port_address as usize);
				for &byte in buf {
					// LF newline characters need to be extended to CRLF over a real serial port.
					if byte == b'\n' {
						unsafe {
							asm!(
								"strb w8, [{port}]",
								port = in(reg) port,
								in("x8") b'\r',
								options(nostack),
							);
						}
					}

					unsafe {
						asm!(
							"strb w8, [{port}]",
							port = in(reg) port,
							in("x8") byte,
							options(nostack),
						);
					}
				}
			}
			#[cfg(feature = "console")]
			SerialInner::Virtio => {
				if let Some(console_driver) = get_console_driver() {
					let _ = console_driver.lock().write(buf);
				}
			}
		}
	}

	pub fn init(&self, _baudrate: u32) {
		// We don't do anything here (yet).
	}
}
