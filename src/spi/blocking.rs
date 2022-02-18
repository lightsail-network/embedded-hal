//! Blocking SPI API
//!
//! # Bus vs Device
//!
//! SPI allows sharing a single bus between many SPI devices. The SCK, MOSI and MISO lines are
//! wired in parallel to all the devices, and each device gets a dedicated chip-select (CS) line from the MCU, like this:
//!
#![doc= include_str!("shared-bus.svg")]
//!
//! CS is usually active-low. When CS is high (not asserted), SPI devices ignore all incoming data, and
//! don't drive MISO. When CS is low (asserted), the device is active: reacts to incoming data on MOSI and
//! drives MISO with the response data. By asserting one CS or another, the MCU can choose to which
//! SPI device it "talks" to on the (possibly shared) bus.
//!
//! This bus sharing is common when having multiple SPI devices in the same board, since it uses fewer MCU
//! pins (`n+3` instead of `4*n`), and fewer MCU SPI peripherals (`1` instead of `n`).
//!
//! However, it poses a challenge when building portable drivers for SPI devices. The driver needs to
//! be able to talk to its device on the bus, while not interfering with other drivers talking to other
//! devices.
//!
//! To solve this, `embedded-hal` has two kinds of SPI traits: **SPI bus** and **SPI device**.
//!
//! ## Bus
//!
//! SPI bus traits represent **exclusive ownership** over the whole SPI bus. This is usually the entire
//! SPI MCU peripheral, plus the SCK, MOSI and MISO pins.
//!
//! Owning an instance of an SPI bus guarantees exclusive access, this is, we have the guarantee no other
//! piece of code will try to use the bus while we own it.
//!
//! There's 3 bus traits, depending on the bus capabilities.
//!
//! - [`SpiBus`]: Read-write access. This is the most commonly used.
//! - [`SpiBusRead`]: Read-only access, for example a bus with a MISO pin but no MOSI pin.
//! - [`SpiBusWrite`]: Read-write access, for example a bus with a MOSI pin but no MISO pin.
//!
//! ## Device
//!
//! [`SpiDevice`] represents **ownership over a single SPI device selected by a CS pin** in a (possibly shared) bus. This is typically:
//!
//! - Exclusive ownership of the **CS pin**.
//! - Access to the **underlying SPI bus**. If shared, it'll be behind some kind of lock/mutex.
//!
//! An [`SpiDevice`] allows initiating [transactions](SpiDevice::transaction) against the target device on the bus. A transaction
//! consists of asserting CS, then doing one or more transfers, then deasserting CS. For the entire duration of the transaction, the [`SpiDevice`]
//! implementation will ensure no other transaction can be opened on the same bus. This is the key that allows correct sharing of the bus.
//!
//! The capabilities of the bus (read-write, read-only or write-only) are determined by which of the [`SpiBus`], [`SpiBusRead`] [`SpiBusWrite`] traits
//! are implemented for the [`Bus`](SpiDevice::Bus) associated type.
//!
//! # For driver authors
//!
//! When implementing a driver, it's crucial to pick the right trait, to ensure correct operation
//! with maximum interoperability. Here are some guidelines depending on the device you're implementing a driver for:
//!
//! If your device **has a CS pin**, use [`SpiDevice`]. Do not manually manage the CS pin, the [`SpiDevice`] implementation will do it for you.
//! Add bounds like `where T::Bus: SpiBus`, `where T::Bus: SpiBusRead`, `where T::Bus: SpiBusWrite` to specify the kind of access you need.
//! By using [`SpiDevice`], your driver will cooperate nicely with other drivers for other devices in the same shared SPI bus.
//!
//! ```
//! # use embedded_hal::spi::blocking::{SpiBus, SpiBusRead, SpiBusWrite, SpiDevice};
//! pub struct MyDriver<SPI> {
//!     spi: SPI,
//! }
//!
//! impl<SPI> MyDriver<SPI>
//! where
//!     SPI: SpiDevice,
//!     SPI::Bus: SpiBus, // or SpiBusRead/SpiBusWrite if you only need to read or only write.
//! {
//!     pub fn new(spi: SPI) -> Self {
//!         Self { spi }
//!     }
//!
//!     pub fn read_foo(&mut self) -> Result<[u8; 2], MyError<SPI::Error>> {
//!         let mut buf = [0; 2];
//!
//!         // `transaction` asserts and deasserts CS for us. No need to do it manually!
//!         self.spi.transaction(|bus| {
//!             bus.write(&[0x90])?;
//!             bus.read(&mut buf)
//!         }).map_err(MyError::Spi)?;
//!
//!         Ok(buf)
//!     }
//! }
//!
//! #[derive(Copy, Clone, Debug)]
//! enum MyError<SPI> {
//!     Spi(SPI),
//!     // Add other errors for your driver here.
//! }
//! ```
//!
//! If your device **does not have a CS pin**, use [`SpiBus`] (or [`SpiBusRead`], [`SpiBusWrite`]). This will ensure
//! your driver has exclusive access to the bus, so no other drivers can interfere. It's not possible to safely share
//! a bus without CS pins. By requiring [`SpiBus`] you disallow sharing, ensuring correct operation.
//!
//! ```
//! # use embedded_hal::spi::blocking::{SpiBus, SpiBusRead, SpiBusWrite};
//! pub struct MyDriver<SPI> {
//!     spi: SPI,
//! }
//!
//! impl<SPI> MyDriver<SPI>
//! where
//!     SPI: SpiBus, // or SpiBusRead/SpiBusWrite if you only need to read or only write.
//! {
//!     pub fn new(spi: SPI) -> Self {
//!         Self { spi }
//!     }
//!
//!     pub fn read_foo(&mut self) -> Result<[u8; 2], MyError<SPI::Error>> {
//!         let mut buf = [0; 2];
//!         self.spi.write(&[0x90]).map_err(MyError::Spi)?;
//!         self.spi.read(&mut buf).map_err(MyError::Spi)?;
//!         Ok(buf)
//!     }
//! }
//!
//! #[derive(Copy, Clone, Debug)]
//! enum MyError<SPI> {
//!     Spi(SPI),
//!     // Add other errors for your driver here.
//! }
//! ```
//!
//! If you're (ab)using SPI to **implement other protocols** by bitbanging (WS2812B, onewire, generating arbitrary waveforms...), use [`SpiBus`].
//! SPI bus sharing doesn't make sense at all in this case. By requiring [`SpiBus`] you disallow sharing, ensuring correct operation.
//!
//! # For HAL authors
//!
//! HALs **must** implement [`SpiBus`], [`SpiBusRead`] and [`SpiBusWrite`]. Users can combine the bus together with the CS pin (which should
//! implement [`OutputPin`]) using HAL-independent [`SpiDevice`] implementations such as [`ExclusiveDevice`].
//!
//! HALs may additionally implement [`SpiDevice`] to **take advantage of hardware CS management**, which may provide some performance
//! benefits. (There's no point in a HAL implementing [`SpiDevice`] if the CS management is software-only, this task is better left to
//! the HAL-independent implementations).
//!
//! HALs **must not** add infrastructure for sharing at the [`SpiBus`] level. User code owning a [`SpiBus`] must have the guarantee
//! of exclusive access.

use core::fmt::Debug;

use crate::{digital::blocking::OutputPin, spi::ErrorType};

use super::{Error, ErrorKind};

/// SPI device trait
///
/// SpiDevice represents ownership over a single SPI device on a (possibly shared) bus, selected
/// with a CS pin.
///
/// See the [module-level documentation](self) for important usage information.
pub trait SpiDevice: ErrorType {
    /// SPI Bus type for this device.
    type Bus: ErrorType;

    /// Perform a transaction against the device.
    ///
    /// - Locks the bus
    /// - Asserts the CS (Chip Select) pin.
    /// - Calls `f` with an exclusive reference to the bus, which can then be used to do transfers against the device.
    /// - Deasserts the CS pin.
    /// - Unlocks the bus.
    ///
    /// The lock mechanism is implementation-defined. The only requirement is it must prevent two
    /// transactions from executing concurrently against the same bus. Examples of implementations are:
    /// critical sections, blocking mutexes, or returning an error or panicking if the bus is already busy.
    fn transaction<R>(
        &mut self,
        f: impl FnOnce(&mut Self::Bus) -> Result<R, <Self::Bus as ErrorType>::Error>,
    ) -> Result<R, Self::Error>;

    /// Do a write within a transaction.
    ///
    /// This is a convenience method equivalent to `device.transaction(|bus| bus.write(buf))`.
    ///
    /// See also: [`SpiDevice::transaction`], [`SpiBusWrite::write`]
    fn write(&mut self, buf: &[u8]) -> Result<(), Self::Error>
    where
        Self::Bus: SpiBusWrite,
    {
        self.transaction(|bus| bus.write(buf))
    }

    /// Do a read within a transaction.
    ///
    /// This is a convenience method equivalent to `device.transaction(|bus| bus.read(buf))`.
    ///
    /// See also: [`SpiDevice::transaction`], [`SpiBusRead::read`]
    fn read(&mut self, buf: &mut [u8]) -> Result<(), Self::Error>
    where
        Self::Bus: SpiBusRead,
    {
        self.transaction(|bus| bus.read(buf))
    }

    /// Do a transfer within a transaction.
    ///
    /// This is a convenience method equivalent to `device.transaction(|bus| bus.transfer(read, write))`.
    ///
    /// See also: [`SpiDevice::transaction`], [`SpiBus::transfer`]
    fn transfer(&mut self, read: &mut [u8], write: &[u8]) -> Result<(), Self::Error>
    where
        Self::Bus: SpiBus,
    {
        self.transaction(|bus| bus.transfer(read, write))
    }

    /// Do an in-place transfer within a transaction.
    ///
    /// This is a convenience method equivalent to `device.transaction(|bus| bus.transfer_in_place(buf))`.
    ///
    /// See also: [`SpiDevice::transaction`], [`SpiBus::transfer_in_place`]
    fn transfer_in_place(&mut self, buf: &mut [u8]) -> Result<(), Self::Error>
    where
        Self::Bus: SpiBus,
    {
        self.transaction(|bus| bus.transfer_in_place(buf))
    }
}

impl<T: SpiDevice> SpiDevice for &mut T {
    type Bus = T::Bus;
    fn transaction<R>(
        &mut self,
        f: impl FnOnce(&mut Self::Bus) -> Result<R, <Self::Bus as ErrorType>::Error>,
    ) -> Result<R, Self::Error> {
        T::transaction(self, f)
    }
}

/// Read-only SPI bus
pub trait SpiBusRead<Word: Copy = u8>: ErrorType {
    /// Reads `words` from the slave.
    ///
    /// The word value sent on MOSI during reading is implementation-defined,
    /// typically `0x00`, `0xFF`, or configurable.
    fn read(&mut self, words: &mut [Word]) -> Result<(), Self::Error>;
}

impl<T: SpiBusRead<Word>, Word: Copy> SpiBusRead<Word> for &mut T {
    fn read(&mut self, words: &mut [Word]) -> Result<(), Self::Error> {
        T::read(self, words)
    }
}

/// Write-only SPI bus
pub trait SpiBusWrite<Word: Copy = u8>: ErrorType {
    /// Writes `words` to the slave, ignoring all the incoming words
    fn write(&mut self, words: &[Word]) -> Result<(), Self::Error>;
}

impl<T: SpiBusWrite<Word>, Word: Copy> SpiBusWrite<Word> for &mut T {
    fn write(&mut self, words: &[Word]) -> Result<(), Self::Error> {
        T::write(self, words)
    }
}

/// Read-write SPI bus
///
/// SpiBus represents **exclusive ownership** over the whole SPI bus, with SCK, MOSI and MISO pins.
///
/// See the [module-level documentation](self) for important information on SPI Bus vs Device traits.
pub trait SpiBus<Word: Copy = u8>: SpiBusRead<Word> + SpiBusWrite<Word> {
    /// Writes and reads simultaneously. `write` is written to the slave on MOSI and
    /// words received on MISO are stored in `read`.
    ///
    /// It is allowed for `read` and `write` to have different lengths, even zero length.
    /// The transfer runs for `max(read.len(), write.len())` words. If `read` is shorter,
    /// incoming words after `read` has been filled will be discarded. If `write` is shorter,
    /// the value of words sent in MOSI after all `write` has been sent is implementation-defined,
    /// typically `0x00`, `0xFF`, or configurable.
    fn transfer(&mut self, read: &mut [Word], write: &[Word]) -> Result<(), Self::Error>;

    /// Writes and reads simultaneously. The contents of `words` are
    /// written to the slave, and the received words are stored into the same
    /// `words` buffer, overwriting it.
    fn transfer_in_place(&mut self, words: &mut [Word]) -> Result<(), Self::Error>;
}

impl<T: SpiBus<Word>, Word: Copy> SpiBus<Word> for &mut T {
    fn transfer(&mut self, read: &mut [Word], write: &[Word]) -> Result<(), Self::Error> {
        T::transfer(self, read, write)
    }

    fn transfer_in_place(&mut self, words: &mut [Word]) -> Result<(), Self::Error> {
        T::transfer_in_place(self, words)
    }
}

/// Error type for [`ExclusiveDevice`] operations.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum ExclusiveDeviceError<BUS, CS> {
    /// An inner SPI bus operation failed
    Spi(BUS),
    /// Asserting or deasserting CS failed
    Cs(CS),
}

impl<BUS, CS> Error for ExclusiveDeviceError<BUS, CS>
where
    BUS: Error + Debug,
    CS: Debug,
{
    fn kind(&self) -> ErrorKind {
        match self {
            Self::Spi(e) => e.kind(),
            Self::Cs(_) => ErrorKind::ChipSelectFault,
        }
    }
}

/// [`SpiDevice`] implementation with exclusive access to the bus (not shared).
///
/// This is the most straightforward way of obtaining an [`SpiDevice`] from an [`SpiBus`],
/// ideal for when no sharing is required (only one SPI device is present on the bus).
pub struct ExclusiveDevice<BUS, CS> {
    bus: BUS,
    cs: CS,
}

impl<BUS, CS> ExclusiveDevice<BUS, CS> {
    /// Create a new ExclusiveDevice
    pub fn new(bus: BUS, cs: CS) -> Self {
        Self { bus, cs }
    }
}

impl<BUS, CS> ErrorType for ExclusiveDevice<BUS, CS>
where
    BUS: ErrorType,
    CS: OutputPin,
{
    type Error = ExclusiveDeviceError<BUS::Error, CS::Error>;
}

impl<BUS, CS> SpiDevice for ExclusiveDevice<BUS, CS>
where
    BUS: ErrorType,
    CS: OutputPin,
{
    type Bus = BUS;

    fn transaction<R>(
        &mut self,
        f: impl FnOnce(&mut Self::Bus) -> Result<R, <Self::Bus as ErrorType>::Error>,
    ) -> Result<R, Self::Error> {
        self.cs.set_low().map_err(ExclusiveDeviceError::Cs)?;

        let f_res = f(&mut self.bus);

        // If the closure fails, it's important to still deassert CS.
        let cs_res = self.cs.set_high();

        let f_res = f_res.map_err(ExclusiveDeviceError::Spi)?;
        cs_res.map_err(ExclusiveDeviceError::Cs)?;

        Ok(f_res)
    }
}
