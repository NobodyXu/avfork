pub mod aspawn;
pub mod syscall;
pub mod lowlevel;
pub mod error;

pub use error::SyscallError;

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
