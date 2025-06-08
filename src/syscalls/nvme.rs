use crate::drivers::nvme::IoQueuePairId;
use crate::drivers::pci::get_nvme_driver;

// TODO: specify vendor_id and device_id to select specific NVMe device
// TODO: document function signature with parameters and return values

pub(crate) enum SysNvmeError {
	ZeroPointerParameter = 1,
	DeviceDoesNotExist = 2,
	CouldNotIdentifyNamespaces = 3,
	NamespaceDoesNotExist = 4,
    MaxNumberOfQueuesReached = 5,
	CouldNotCreateIoQueuePair = 6,
	CouldNotDeleteIoQueuePair = 7,
	CouldNotFindIoQueuePair = 8,
	BufferTooBig = 9,
	CouldNotAllocateMemory = 10,
	CouldNotReadFromIoQueuePair = 11,
	CouldNotWriteToIoQueuePair = 12,
}

#[hermit_macro::system]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sys_nvme_get_number_of_namespaces(result: *mut usize) -> usize {
	fn inner(result: *mut usize) -> Result<(), SysNvmeError> {
		if result.is_null() {
			return Err(SysNvmeError::ZeroPointerParameter);
		}
		let result = unsafe { &mut *result };
		let driver = get_nvme_driver().ok_or(SysNvmeError::DeviceDoesNotExist)?;
		let number_of_namespaces = driver.lock().get_number_of_namespaces()?;
		*result = number_of_namespaces;
		Ok(())
	}
	match inner(result) {
		Ok(()) => 0,
		Err(error) => error as usize,
	}
}

#[hermit_macro::system]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sys_nvme_get_max_buffer_size(result: *mut usize) -> usize {
	fn inner(result: *mut usize) -> Result<(), SysNvmeError> {
		if result.is_null() {
			return Err(SysNvmeError::ZeroPointerParameter);
		}
		let result = unsafe { &mut *result };
		let driver = get_nvme_driver().ok_or(SysNvmeError::DeviceDoesNotExist)?;
		let max_buffer_size = driver.lock().get_max_buffer_size();
		*result = max_buffer_size;
		Ok(())
	}
	match inner(result) {
		Ok(()) => 0,
		Err(error) => error as usize,
	}
}

#[hermit_macro::system]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sys_nvme_get_max_number_of_queue_entries(result: *mut u16) -> usize {
	fn inner(result: *mut u16) -> Result<(), SysNvmeError> {
		if result.is_null() {
			return Err(SysNvmeError::ZeroPointerParameter);
		}
		let result = unsafe { &mut *result };
		let driver = get_nvme_driver().ok_or(SysNvmeError::DeviceDoesNotExist)?;
		let max_number_of_queue_entries = driver.lock().get_max_number_of_queue_entries();
		*result = max_number_of_queue_entries;
		Ok(())
	}
	match inner(result) {
		Ok(()) => 0,
		Err(error) => error as usize,
	}
}

#[hermit_macro::system]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sys_nvme_get_size_of_namespace(
	namespace_index: usize,
	result: *mut u64,
) -> usize {
	fn inner(namespace_index: usize, result: *mut u64) -> Result<(), SysNvmeError> {
		if result.is_null() {
			return Err(SysNvmeError::ZeroPointerParameter);
		}
		let result = unsafe { &mut *result };
		let driver = get_nvme_driver().ok_or(SysNvmeError::DeviceDoesNotExist)?;
		let size_of_namespace = driver.lock().get_size_of_namespace(namespace_index)?;
		*result = size_of_namespace;
		Ok(())
	}
	match inner(namespace_index, result) {
		Ok(()) => 0,
		Err(error) => error as usize,
	}
}

#[hermit_macro::system]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sys_nvme_create_io_queue_pair(
	namespace_index: usize,
	number_of_entries: u16,
	resulting_io_queue_pair_id: *mut usize,
) -> usize {
	fn inner(
		namespace_index: usize,
		number_of_entries: u16,
		resulting_io_queue_pair_id: *mut usize,
	) -> Result<(), SysNvmeError> {
		if resulting_io_queue_pair_id.is_null() {
			return Err(SysNvmeError::ZeroPointerParameter);
		}
		let resulting_io_queue_pair_id = unsafe { &mut *resulting_io_queue_pair_id };
		let driver = get_nvme_driver().ok_or(SysNvmeError::DeviceDoesNotExist)?;
		let io_queue_pair_id = driver
			.lock()
			.create_io_queue_pair(namespace_index, number_of_entries)?;
		*resulting_io_queue_pair_id = io_queue_pair_id.into();
		Ok(())
	}
	match inner(
		namespace_index,
		number_of_entries,
		resulting_io_queue_pair_id,
	) {
		Ok(()) => 0,
		Err(error) => error as usize,
	}
}

#[hermit_macro::system]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sys_nvme_delete_io_queue_pair(io_queue_pair_id: usize) -> usize {
	fn inner(io_queue_pair_id: usize) -> Result<(), SysNvmeError> {
		let driver = get_nvme_driver().ok_or(SysNvmeError::DeviceDoesNotExist)?;
		driver
			.lock()
			.delete_io_queue_pair(IoQueuePairId::from(io_queue_pair_id))
	}
	match inner(io_queue_pair_id) {
		Ok(()) => 0,
		Err(error) => error as usize,
	}
}

#[hermit_macro::system]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sys_nvme_read_from_io_queue_pair(
	io_queue_pair_id: usize,
	buffer_pointer: *mut u8,
	buffer_size: usize,
	logical_block_address: u64,
) -> usize {
	fn inner(
		io_queue_pair_id: usize,
		buffer_pointer: *mut u8,
		buffer_size: usize,
		logical_block_address: u64,
	) -> Result<(), SysNvmeError> {
		let buffer = unsafe { core::slice::from_raw_parts_mut(buffer_pointer, buffer_size) };
		let driver = get_nvme_driver().ok_or(SysNvmeError::DeviceDoesNotExist)?;
		driver.lock().read_from_io_queue_pair(
			&IoQueuePairId::from(io_queue_pair_id),
			buffer,
			logical_block_address,
		)
	}
	match inner(
		io_queue_pair_id,
		buffer_pointer,
		buffer_size,
		logical_block_address,
	) {
		Ok(()) => 0,
		Err(error) => error as usize,
	}
}

#[hermit_macro::system]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sys_nvme_write_to_io_queue_pair(
	io_queue_pair_id: usize,
	buffer_pointer: *const u8,
	buffer_size: usize,
	logical_block_address: u64,
) -> usize {
	fn inner(
		io_queue_pair_id: usize,
		buffer_pointer: *const u8,
		buffer_size: usize,
		logical_block_address: u64,
	) -> Result<(), SysNvmeError> {
		let buffer = unsafe { core::slice::from_raw_parts(buffer_pointer, buffer_size) };
		let driver = get_nvme_driver().ok_or(SysNvmeError::DeviceDoesNotExist)?;
		driver.lock().write_to_io_queue_pair(
			&IoQueuePairId::from(io_queue_pair_id),
			buffer,
			logical_block_address,
		)
	}
	match inner(
		io_queue_pair_id,
		buffer_pointer,
		buffer_size,
		logical_block_address,
	) {
		Ok(()) => 0,
		Err(error) => error as usize,
	}
}
