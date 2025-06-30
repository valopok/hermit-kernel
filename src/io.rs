use embedded_io::{Error, ErrorKind};

use crate::errno::Errno;

// TODO: Integrate with src/errno.rs ?
#[allow(clippy::upper_case_acronyms)]
#[derive(TryFromPrimitive, IntoPrimitive, PartialEq, Eq, Clone, Copy, Debug)]
#[repr(i32)]
pub enum Error {
	ENOENT = 2,
	ENOSYS = 38,
	EIO = 5,
	EBADF = 9,
	EISDIR = 21,
	EINVAL = 22,
	ETIME = 62,
	EAGAIN = 11,
	EFAULT = 14,
	ENOBUFS = 105,
	ENOTCONN = 107,
	ENOTDIR = 20,
	EMFILE = 24,
	EEXIST = 17,
	EADDRINUSE = 98,
	EOVERFLOW = 75,
	ENOTSOCK = 88,
}

impl From<Errno> for ErrorKind {
	fn from(value: Errno) -> Self {
		match value {
			Errno::Noent => ErrorKind::NotFound,
			Errno::Acces | Errno::Perm => ErrorKind::PermissionDenied,
			Errno::Connrefused => ErrorKind::ConnectionRefused,
			Errno::Connreset => ErrorKind::ConnectionReset,
			Errno::Connaborted => ErrorKind::ConnectionAborted,
			Errno::Notconn => ErrorKind::NotConnected,
			Errno::Addrinuse => ErrorKind::AddrInUse,
			Errno::Addrnotavail => ErrorKind::AddrNotAvailable,
			Errno::Pipe => ErrorKind::BrokenPipe,
			Errno::Exist => ErrorKind::AlreadyExists,
			Errno::Inval => ErrorKind::InvalidInput,
			Errno::Timedout => ErrorKind::TimedOut,
			Errno::Intr => ErrorKind::Interrupted,
			Errno::Opnotsupp => ErrorKind::Unsupported,
			Errno::Nomem => ErrorKind::OutOfMemory,
			_ => ErrorKind::Other,
		}
	}
}

impl Error for Errno {
	fn kind(&self) -> ErrorKind {
		(*self).into()
	}
}
