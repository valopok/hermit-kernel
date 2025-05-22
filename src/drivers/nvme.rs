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
		let mut controller =
			nvme::Device::init(virtual_address.as_usize(), allocator).map_err(|_| ())?;
		debug!("NVMe controller data: {:?}", controller.controller_data());

		let namespaces = controller
			.identify_namespaces(0)
			.expect("NVMe driver: could not identify name spaces.");

		let namespace = &namespaces[0];
		let _disk_size = namespace.block_count() * namespace.block_size();

		let mut qpair = controller
			.create_io_queue_pair(namespace.clone(), 64)
			.expect("NVMe driver: could not create io queue pair.");

		const BUFFER_SIZE: usize = 16;
        assert!(BUFFER_SIZE <= controller.controller_data().max_transfer_size);

        let allocator = DeviceAlloc {};
        let layout = unsafe { Layout::from_size_align_unchecked(BUFFER_SIZE, BasePageSize::SIZE as usize) };

		let mut pointer = allocator.allocate(layout).expect("NVMe driver: could not allocate buffer.");
		let buffer_1: &mut [u8] = unsafe { pointer.as_mut() };

		let mut pointer = allocator.allocate(layout).expect("NVMe driver: could not allocate buffer.");
		let buffer_2: &mut [u8] = unsafe { pointer.as_mut() };

		// Fill buffer 1 with data
		for i in 0..layout.size() {
			buffer_1[i] = (i % 256) as u8;
		}

		// Write buffer 1 to the disk starting from the given Logical Block Address
		qpair
			.write(buffer_1.as_ptr(), buffer_1.len(), 0)
			.expect("NVMe driver: could not write the buffer.");

		// Read the written data to buffer 2 from the given Logical Block Address
		qpair
			.read(buffer_2.as_mut_ptr(), buffer_2.len(), 0)
			.expect("NVMe driver: could not read to the buffer.");

		// Verify the data byte-by-byte
		for (i, (read, write)) in buffer_1.iter().zip(buffer_2.iter()).enumerate() {
			if read != write {
				error!("Write test: Mismatch at index {i}: {read} != {write}");
				break;
			}
		}

		// Delete the I/O queue pair to release resources
		controller
			.delete_io_queue_pair(qpair)
			.expect("NVMe driver: could not delete io queue pair.");

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
