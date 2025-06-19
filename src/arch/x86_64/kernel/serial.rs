use alloc::collections::VecDeque;
use alloc::vec::Vec;

use crate::arch::x86_64::kernel::apic;
use crate::arch::x86_64::kernel::core_local::increment_irq_counter;
use crate::arch::x86_64::kernel::interrupts::{self, IDT};
#[cfg(all(feature = "pci", feature = "console"))]
use crate::drivers::pci::get_console_driver;
use crate::executor::WakerRegistration;
use crate::syscalls::interfaces::serial_buf_hypercall;

#[cfg(feature = "pci")]
use crate::arch::x86_64::kernel::interrupts;
#[cfg(feature = "pci")]
use crate::drivers::InterruptLine;
use crate::errno::Errno;

enum SerialInner {
	Uart(uart_16550::SerialPort),
	Uhyve,
	#[cfg(all(feature = "console", feature = "pci"))]
	Virtio,
}

impl UartDevice {
	pub unsafe fn new() -> Self {
		let base = crate::env::boot_info()
			.hardware_info
			.serial_port_base
			.unwrap()
			.get();
		let mut uart = unsafe { uart_16550::SerialPort::new(base) };
		uart.init();

		Self {
			uart,
			buffer: VecDeque::new(),
		}
	}
}

pub(crate) struct SerialDevice;

impl SerialDevice {
	pub fn new() -> Self {
		Self {}
	}
}

impl ErrorType for SerialDevice {
	type Error = Errno;
}

impl Read for SerialDevice {
	fn read(&mut self, buf: &mut [u8]) -> Result<usize, Self::Error> {
		let mut guard = UART_DEVICE.lock();
		if guard.buffer.is_empty() {
			Ok(0)
		} else {
			let mut serial = unsafe { uart_16550::SerialPort::new(base) };
			serial.init();
			Self {
				inner: SerialInner::Uart(serial),
				buffer: VecDeque::new(),
				waker: WakerRegistration::new(),
			}
		}
	}

	pub fn buffer_input(&mut self) {
		if let SerialInner::Uart(s) = &mut self.inner {
			let c = s.receive();
			if c == b'\r' {
				self.buffer.push_back(b'\n');
			} else {
				self.buffer.push_back(c);
			}
			self.waker.wake();
		}
	}

	pub fn register_waker(&mut self, waker: &Waker) {
		self.waker.register(waker);
	}

	pub fn read(&mut self) -> Option<u8> {
		self.buffer.pop_front()
	}

	pub fn is_empty(&self) -> bool {
		self.buffer.is_empty()
	}

	pub fn send(&mut self, buf: &[u8]) {
		match &mut self.inner {
			SerialInner::Uhyve => serial_buf_hypercall(buf),
			SerialInner::Uart(s) => {
				for &data in buf {
					s.send(data);
				}
			}
			#[cfg(all(feature = "console", feature = "pci"))]
			SerialInner::Virtio => {
				if let Some(console_driver) = get_console_driver() {
					let _ = console_driver.lock().write(buf);
				}
			}
		}
	}

	#[cfg(all(feature = "pci", feature = "console"))]
	pub fn switch_to_virtio_console(&mut self) {
		self.inner = SerialInner::Virtio;
	}
}

impl ReadReady for SerialDevice {
	fn read_ready(&mut self) -> Result<bool, Self::Error> {
		let read_ready = !UART_DEVICE.lock().buffer.is_empty();
		Ok(read_ready)
	}
}

impl Write for SerialDevice {
	fn write(&mut self, buf: &[u8]) -> Result<usize, Self::Error> {
		let mut guard = UART_DEVICE.lock();

		for &data in buf {
			guard.uart.send(data);
		}

		Ok(buf.len())
	}

	fn flush(&mut self) -> Result<(), Self::Error> {
		Ok(())
	}
}

#[cfg(feature = "pci")]
pub(crate) fn get_serial_handler() -> (InterruptLine, fn()) {
	fn serial_handler() {
		let mut guard = UART_DEVICE.lock();
		if let Ok(c) = guard.uart.try_receive() {
			guard.buffer.push_back(c);
		}

		drop(guard);
		crate::console::CONSOLE_WAKER.lock().wake();
	}

	interrupts::add_irq_name(SERIAL_IRQ, "COM1");

	(SERIAL_IRQ, serial_handler)
}
