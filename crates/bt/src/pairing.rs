//! Objective-C delegate for `IOBluetoothDevicePair`. Callbacks arrive on the pairing
//! thread's run loop and are forwarded over a channel to the loop in `backend::pair`.

use std::sync::mpsc::Sender;

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObject, NSObjectProtocol};
use objc2::{define_class, msg_send, AllocAnyThread, DefinedClass};
use objc2_io_bluetooth::IOBluetoothDevicePairDelegate;

pub enum Req {
    Pin,
    Confirm(u32),
    Finished(i32),
}

pub struct Ivars {
    tx: Sender<Req>,
}

define_class!(
    #[unsafe(super(NSObject))]
    #[ivars = Ivars]
    pub struct PairDelegate;

    unsafe impl NSObjectProtocol for PairDelegate {}

    unsafe impl IOBluetoothDevicePairDelegate for PairDelegate {
        #[unsafe(method(devicePairingPINCodeRequest:))]
        fn pin_request(&self, _sender: Option<&AnyObject>) {
            let _ = self.ivars().tx.send(Req::Pin);
        }

        #[unsafe(method(devicePairingUserConfirmationRequest:numericValue:))]
        fn confirm_request(&self, _sender: Option<&AnyObject>, value: u32) {
            let _ = self.ivars().tx.send(Req::Confirm(value));
        }

        #[unsafe(method(devicePairingFinished:error:))]
        fn finished(&self, _sender: Option<&AnyObject>, error: i32) {
            let _ = self.ivars().tx.send(Req::Finished(error));
        }
    }
);

impl PairDelegate {
    pub fn new(tx: Sender<Req>) -> Retained<Self> {
        let this = Self::alloc().set_ivars(Ivars { tx });
        unsafe { msg_send![super(this), init] }
    }
}
