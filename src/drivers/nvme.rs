use ahash::RandomState;
use hashbrown::HashMap;
use hermit_sync::{InterruptTicketMutex, Lazy};
use memory_addresses::VirtAddr;
use nvme::{Allocator, Device};

use crate::arch::mm::paging::virtual_to_physical;
use crate::arch::pci::PciConfigRegion;
use crate::drivers::pci::PciDevice;
use crate::mm::virtualmem;

pub(crate) struct NvmeDriver {}

impl NvmeDriver {
	pub(crate) fn init(device: &PciDevice<PciConfigRegion>) -> Result<Self, ()> {
		let allocator: NvmeAllocator = NvmeAllocator {
			allocations: Lazy::new(|| {
				InterruptTicketMutex::new(HashMap::with_hasher(RandomState::with_seeds(0, 0, 0, 0)))
			}),
		};
		for i in 0..pci_types::MAX_BARS {
			debug!("Bar {}: {:?}", i, device.get_bar(i as u8));
		}
		let (virtual_address, _size) = device.memory_map_bar(0, true).ok_or(())?;
		debug!(
			"Memory map bar: virtual_address {:#x}, size {:#x}",
			virtual_address, _size
		);
		let controller = Device::init(virtual_address.as_usize(), allocator).map_err(|_| ())?;
		debug!("NVMe controller data: {:?}", controller.controller_data());
		Ok(Self {})
	}
}

pub(crate) struct NvmeAllocator {
	// TODO: Replace with a concurrent hashmap.
	pub allocations: Lazy<InterruptTicketMutex<HashMap<VirtAddr, usize, RandomState>>>,
}

impl Allocator for NvmeAllocator {
	// returns the virtual address as usize
	unsafe fn allocate(&self, size: usize) -> usize {
		debug!("NVMe: allocate size {:#x}", size);
		let virtual_address: VirtAddr =
			virtualmem::allocate(size).expect("Could not allocate virtual memory.");
		self.allocations.lock().insert(virtual_address, size);
		virtual_address.as_usize()
	}

	unsafe fn deallocate(&self, address: usize) {
		debug!("NVMe: deallocate address {:#x}", address);
		let virtual_address: VirtAddr = VirtAddr::new(address as u64);
		let size: usize = self.allocations
			.lock()
            .remove(&virtual_address)
			.expect("The given address did not map to a size. This mapping should have occured during allocation.");
		virtualmem::deallocate(virtual_address, size);
	}

	fn translate(&self, address: usize) -> usize {
		debug!("NVMe: translate virtual address {:#x}", address);
		let virtual_address: VirtAddr = VirtAddr::new(address as u64);
		virtual_to_physical(virtual_address)
			.expect("The given virtual address could not be mapped to a physical one.")
			.as_usize()
	}
}
