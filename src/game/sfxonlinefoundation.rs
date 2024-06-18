use super::core::{FName, FScriptDelegate, TArray, UObject};
use std::os::raw::{c_ulong, c_void};

// Class SFXOnlineFoundation.SFXOnlineComponent
// 0x0028 (0x0064 - 0x003C)
#[derive(Debug, Clone, Copy)]
#[repr(C, packed(4))]
pub struct USFXOnlineComponent {
    pub _base: UObject,
    pub event_subscriber_table: TArray<FSFXOnlineSubscriberEventType>,
    pub __on_event_delegate: FScriptDelegate,
    pub api_name: FName,
    pub online_subsystem: *mut c_void, /* USFXOnlineSubsystem */
    pub needs_state_machine: c_ulong,
}

#[derive(Debug, Clone, Copy)]
#[repr(C, packed(4))]
pub struct FSFXOnlineSubscriberEventType {
    pub event_callback: FName,
    pub event_type: ::std::os::raw::c_uchar,
}
