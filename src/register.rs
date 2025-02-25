/// Trait for accessing a 16550's register
pub trait Uart16550Register {
    #[allow(missing_docs)]
    fn read(&self) -> u8;
    #[allow(missing_docs)]
    fn write(&mut self, value: u8);
}
