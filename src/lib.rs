#![no_std]

pub mod intrusive;

pub mod channel;
pub mod sync;
pub mod timer;

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
