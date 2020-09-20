pub(crate) mod intrusive;

pub mod sync;
pub mod channel;

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
