use core::alloc::{Allocator, Layout};
use core::ptr::NonNull;

use ahash::RandomState;
use hashbrown::HashMap;
use hermit_sync::{InterruptTicketMutex, Lazy};
use memory_addresses::VirtAddr;
use pci_types::InterruptLine;

use crate::arch::mm::paging::{virtual_to_physical, BasePageSize, PageSize};
use crate::arch::pci::PciConfigRegion;
use crate::drivers::pci::PciDevice;
use crate::drivers::Driver;
use crate::mm::device_alloc::DeviceAlloc;

pub(crate) struct NvmeDriver {
	irq: InterruptLine,
}

impl NvmeDriver {
	pub(crate) fn init(device: &PciDevice<PciConfigRegion>) -> Result<Self, ()> {
		let allocator: NvmeAllocator = NvmeAllocator {
			device_allocator: DeviceAlloc {},
			allocations: Lazy::new(|| {
				InterruptTicketMutex::new(HashMap::with_hasher(RandomState::with_seeds(0, 0, 0, 0)))
			}),
		};
		let (virtual_address, _size) = device.memory_map_bar(0, true).ok_or(())?;
		let controller =
			nvme::Device::init(virtual_address.as_usize(), allocator).map_err(|_| ())?;
		debug!("NVMe controller data: {:?}", controller.controller_data());
		Ok(Self {
			irq: device
				.get_irq()
				.expect("NVMe driver: Could not get irq from device."),
		})
	}
}

pub(crate) struct NvmeAllocator {
	pub device_allocator: DeviceAlloc,
	// TODO: Replace with a concurrent hashmap. See crate::synch::futex.
	pub allocations: Lazy<InterruptTicketMutex<HashMap<usize, (NonNull<u8>, Layout), RandomState>>>,
}

impl nvme::Allocator for NvmeAllocator {
	// returns the virtual address as usize
	unsafe fn allocate(&self, size: usize) -> usize {
		debug!("NVMe driver: allocate size {:#x}", size);
		let layout: Layout =
			unsafe { Layout::from_size_align_unchecked(size, BasePageSize::SIZE as usize) };
		let memory = self
			.device_allocator
			.allocate(layout)
			.expect("NVMe driver: Could not allocate memory with device allocator.");
		self.allocations
			.lock()
			.insert(memory.as_ptr().addr(), (memory.as_non_null_ptr(), layout));
		memory.as_ptr().addr()
	}

	unsafe fn deallocate(&self, address: usize) {
		debug!("NVMe driver: deallocate address {:#x}", address);
		let (virtual_address, layout): (NonNull<u8>, Layout) = self.allocations
			.lock()
            .remove(&address)
			.expect("NVMe driver: The given address did not map to an address and a layout. This mapping should have occured during allocation.");
		unsafe { self.device_allocator.deallocate(virtual_address, layout) }
	}

	fn translate(&self, address: usize) -> usize {
		debug!("NVMe driver: translate virtual address {:#x}", address);
		let virtual_address: VirtAddr = VirtAddr::new(address as u64);
		virtual_to_physical(virtual_address)
			.expect("NVMe driver: The given virtual address could not be mapped to a physical one.")
			.as_usize()
	}
}

impl Driver for NvmeDriver {
	fn get_interrupt_number(&self) -> InterruptLine {
		self.irq
	}

	fn get_name(&self) -> &'static str {
		"nvme"
	}
}
