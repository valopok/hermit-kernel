use core::alloc::{Allocator, Layout};
use core::ptr::NonNull;

use ahash::RandomState;
use hashbrown::HashMap;
use hermit_sync::{InterruptTicketMutex, Lazy};
use memory_addresses::VirtAddr;
use nvme;
use pci_types::InterruptLine;

use crate::alloc::borrow::ToOwned;
use crate::arch::mm::paging::{virtual_to_physical, BasePageSize, PageSize};
use crate::arch::pci::PciConfigRegion;
use crate::drivers::pci::PciDevice;
use crate::drivers::Driver;
use crate::mm::device_alloc::DeviceAlloc;
use crate::syscalls::nvme::SysNvmeError;

pub(crate) struct NvmeDriver {
	irq: InterruptLine,
	// vendor_id: u16,
	// device_id: u16,
	controller: nvme::Device<NvmeAllocator>,
	// TODO: Replace with a concurrent hashmap. See crate::synch::futex.
	io_queue_pairs:
		Lazy<InterruptTicketMutex<HashMap<usize, nvme::IoQueuePair<NvmeAllocator>, RandomState>>>,
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
		let controller: nvme::Device<NvmeAllocator> =
			nvme::Device::init(virtual_address.as_usize(), allocator).map_err(|_| ())?;
		debug!("NVMe controller data: {:?}", controller.controller_data());

		// let (vendor_id, device_id) = device.id();
		Ok(Self {
			irq: device
				.get_irq()
				.expect("NVMe driver: Could not get irq from device."),
			controller,
			io_queue_pairs: Lazy::new(|| {
				InterruptTicketMutex::new(HashMap::with_hasher(RandomState::with_seeds(0, 0, 0, 0)))
			}),
		})
	}

	pub(crate) fn get_number_of_namespaces(&mut self) -> Result<usize, SysNvmeError> {
		self.controller
			.identify_namespaces(0)
			.map(|nss| nss.len())
			.map_err(|_| SysNvmeError::CouldNotIdentifyNamespaces)
	}

	/// Gets the size of a namespace in bytes.
	pub(crate) fn get_size_of_namespace(
		&mut self,
		namespace_index: usize,
	) -> Result<u64, SysNvmeError> {
		let namespaces = self
			.controller
			.identify_namespaces(0)
			.map_err(|_| SysNvmeError::CouldNotIdentifyNamespaces)?;
		let namespace = namespaces
			.get(namespace_index)
			.ok_or(SysNvmeError::NamespaceDoesNotExist)?;
		Ok(namespace.block_count() * namespace.block_size())
	}

	/// Creates an IO queue pair with a number of entries for a namespace and returns its ID.
	pub(crate) fn create_io_queue_pair(
		&mut self,
		namespace_index: usize,
		number_of_entries: usize,
	) -> Result<usize, SysNvmeError> {
		let namespaces = self
			.controller
			.identify_namespaces(0)
			.map_err(|_| SysNvmeError::CouldNotIdentifyNamespaces)?;
		let namespace = namespaces
			.get(namespace_index)
			.ok_or(SysNvmeError::NamespaceDoesNotExist)?;
		let io_queue_pair = self
			.controller
			.create_io_queue_pair(namespace.to_owned(), number_of_entries)
			.map_err(|_| SysNvmeError::CouldNotCreateIoQueuePair)?;
		let mut io_queue_pairs = self.io_queue_pairs.lock();
		// Simple way to avoid collisions while reusing some previously deleted keys.
		// This can definitely be improved.
		let min = io_queue_pairs
			.keys()
			.min()
			.map(|m| m.to_owned())
			.unwrap_or(0);
		let max = io_queue_pairs
			.keys()
			.max()
			.map(|m| m.to_owned())
			.unwrap_or(0);
		let index;
		if min != 0 {
			index = min - 1;
		} else {
			index = max + 1;
		}
		io_queue_pairs.insert(index, io_queue_pair);
		Ok(index)
	}

	/// Deletes an IO queue pair and frees its resources.
	pub(crate) fn delete_io_queue_pair(
		&mut self,
		io_queue_pair_id: usize,
	) -> Result<(), SysNvmeError> {
		let io_queue_pair = self
			.io_queue_pairs
			.lock()
			.remove(&io_queue_pair_id)
			.ok_or(SysNvmeError::CouldNotFindIoQueuePair)?;
		self.controller
			.delete_io_queue_pair(io_queue_pair)
			.map_err(|_| SysNvmeError::CouldNotDeleteIoQueuePair)
	}

	/// Reads from an IO queue pair into a buffer starting from a Logical Block Address.
	pub(crate) fn read_from_io_queue_pair(
		&mut self,
		io_queue_pair_id: usize,
		buffer: &mut [u8],
		logical_block_address: u64,
	) -> Result<(), SysNvmeError> {
		let mut io_queue_pairs = self.io_queue_pairs.lock();
		let io_queue_pair = io_queue_pairs
			.get_mut(&io_queue_pair_id)
			.ok_or(SysNvmeError::CouldNotFindIoQueuePair)?;
		if buffer.len() > self.controller.controller_data().max_transfer_size {
			return Err(SysNvmeError::BufferTooBig);
		}

		let layout = Layout::from_size_align(buffer.len(), BasePageSize::SIZE as usize)
			.map_err(|_| SysNvmeError::BufferTooBig)?;
		let mut pointer = DeviceAlloc {}
			.allocate(layout)
			.map_err(|_| SysNvmeError::CouldNotAllocateMemory)?;
		let kernel_buffer: &mut [u8] = unsafe { pointer.as_mut() };

		io_queue_pair
			.read(
				kernel_buffer.as_mut_ptr(),
				kernel_buffer.len(),
				logical_block_address,
			)
			.map_err(|_| SysNvmeError::CouldNotReadFromIoQueuePair)?;

		buffer.copy_from_slice(&kernel_buffer[0..buffer.len()]);
		Ok(())
	}

	/// Writes a buffer to an IO queue pair starting from a Logical Block Address.
	pub(crate) fn write_to_io_queue_pair(
		&mut self,
		io_queue_pair_id: usize,
		buffer: &[u8],
		logical_block_address: u64,
	) -> Result<(), SysNvmeError> {
		let mut io_queue_pairs = self.io_queue_pairs.lock();
		let io_queue_pair = io_queue_pairs
			.get_mut(&io_queue_pair_id)
			.ok_or(SysNvmeError::CouldNotFindIoQueuePair)?;
		if buffer.len() > self.controller.controller_data().max_transfer_size {
			return Err(SysNvmeError::BufferTooBig);
		}

		let layout = Layout::from_size_align(buffer.len(), BasePageSize::SIZE as usize)
			.map_err(|_| SysNvmeError::BufferTooBig)?;
		let mut pointer = DeviceAlloc {}
			.allocate(layout)
			.map_err(|_| SysNvmeError::CouldNotAllocateMemory)?;
		let kernel_buffer: &mut [u8] = unsafe { pointer.as_mut() };
		kernel_buffer[0..buffer.len()].copy_from_slice(buffer);

		io_queue_pair
			.write(
				kernel_buffer.as_ptr(),
				kernel_buffer.len(),
				logical_block_address,
			)
			.map_err(|_| SysNvmeError::CouldNotWriteToIoQueuePair)?;
		Ok(())
	}
}

pub(crate) struct NvmeAllocator {
	pub(crate) device_allocator: DeviceAlloc,
	// TODO: Replace with a concurrent hashmap. See crate::synch::futex.
	pub(crate) allocations: Lazy<InterruptTicketMutex<HashMap<usize, Layout, RandomState>>>,
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
			.insert(memory.as_ptr().addr(), layout);
		memory.as_ptr().addr()
	}

	unsafe fn deallocate(&self, address: usize) {
		debug!("NVMe driver: deallocate address {:#x}", address);
		let layout: Layout = self.allocations
			.lock()
            .remove(&address)
			.expect("NVMe driver: The given address did not map to an address and a layout. This mapping should have occured during allocation.");
		let virtual_address = unsafe { NonNull::new_unchecked(address as *mut _) };
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
