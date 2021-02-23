#[doc(hidden)]
#[macro_export]
macro_rules! ioerr {
    (@E $kind:expr, $arg:expr) => {
        ::std::io::Error::new($kind, $arg)
    };

    (NotFound $($tt:tt)*) => ( ioerr!(::std::io::ErrorKind::NotFound $($tt)*) );
    (PermissionDenied $($tt:tt)*) => ( ioerr!(::std::io::ErrorKind::PermissionDenied $($tt)*) );
    (ConnectionRefused $($tt:tt)*) => ( ioerr!(::std::io::ErrorKind::ConnectionRefused $($tt)*) );
    (ConnectionReset $($tt:tt)*) => ( ioerr!(::std::io::ErrorKind::ConnectionReset $($tt)*) );
    (ConnectionAborted $($tt:tt)*) => ( ioerr!(::std::io::ErrorKind::ConnectionAborted $($tt)*) );
    (NotConnected $($tt:tt)*) => ( ioerr!(::std::io::ErrorKind::NotConnected $($tt)*) );
    (AddrInUse $($tt:tt)*) => ( ioerr!(::std::io::ErrorKind::AddrInUse $($tt)*) );
    (AddrNotAvailable $($tt:tt)*) => ( ioerr!(::std::io::ErrorKind::AddrNotAvailable $($tt)*) );
    (BrokenPipe $($tt:tt)*) => ( ioerr!(::std::io::ErrorKind::BrokenPipe $($tt)*) );
    (AlreadyExists $($tt:tt)*) => ( ioerr!(::std::io::ErrorKind::AlreadyExists $($tt)*) );
    (WouldBlock $($tt:tt)*) => ( ioerr!(::std::io::ErrorKind::WouldBlock $($tt)*) );
    (InvalidInput $($tt:tt)*) => ( ioerr!(::std::io::ErrorKind::InvalidInput $($tt)*) );
    (InvalidData $($tt:tt)*) => ( ioerr!(::std::io::ErrorKind::InvalidData $($tt)*) );
    (TimedOut $($tt:tt)*) => ( ioerr!(::std::io::ErrorKind::TimedOut $($tt)*) );
    (WriteZero $($tt:tt)*) => ( ioerr!(::std::io::ErrorKind::WriteZero $($tt)*) );
    (Interrupted $($tt:tt)*) => ( ioerr!(::std::io::ErrorKind::Interrupted $($tt)*) );
    (Other $($tt:tt)*) => ( ioerr!(::std::io::ErrorKind::Other $($tt)*) );
    (UnexpectedEof $($tt:tt)*) => ( ioerr!(::std::io::ErrorKind::UnexpectedEof $($tt)*) );

    ($kind:path, $fmt:expr, $($tt:tt)+) => (
        ioerr!(@E $kind, format!($fmt, $($tt)+))
    );
    ($kind:expr, $fmt:expr, $($tt:tt)+) => (
        ioerr!(@E $kind, format!($fmt, $($tt)+))
    );
    ($kind:path, $arg:expr) => (
        ioerr!(@E $kind, $arg)
    );
    ($kind:expr, $arg:expr) => (
        ioerr!(@E $kind, $arg)
    );
    ($kind:path) => (
        ::std::io::Error::from($kind)
    );
    ($kind:expr) => (
        ::std::io::Error::from($kind)
    );
}
