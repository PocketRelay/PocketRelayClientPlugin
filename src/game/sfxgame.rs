use super::{core::FString, sfxonlinefoundation::USFXOnlineComponent};
use crate::{
    game::core::{get_function_object, UFunction},
    hooks::process_event::process_event,
};
use std::ptr::null_mut;

macro_rules! define_method {
    ($func_name:ident, $fn_index:expr, $( $arg_name:ident : $arg_type:ty ),*) => {
        #[allow(clippy::missing_safety_doc)]
        pub unsafe fn $func_name(
            &mut self,
            $( $arg_name: $arg_type ),*
        ) {
            /// Generated structure to hold the function params
            #[repr(C)]
            #[allow(non_camel_case_types)]
            struct Params {
                $( $arg_name: $arg_type ),*
            }

            static mut FN_PTR: *mut UFunction = null_mut();

            // Create the function object pointer if not initialized
            if FN_PTR.is_null() {
                if let Some(fn_ptr) = get_function_object($fn_index) {
                    FN_PTR = fn_ptr;
                } else {
                    panic!("Missing {} ({}) function object", stringify!($func_name), stringify!($fn_index))
                }
            }

            // Create the function params
            let mut params = Params {
                $( $arg_name ),*
            };

            process_event(
                self as *const _ as *mut _,
                FN_PTR,
                &mut params as *const _ as *mut _,
                std::ptr::null_mut(),
            );
        }
    };
}

#[repr(C, packed(4))]
pub struct USFXOnlineComponentUI {
    // Base C++ class for this class
    pub _base: USFXOnlineComponent,
    // struct FPointer                                    VfTable_IISFXOnlineComponentUserInterface;        		// 0x0064 (0x0004) [0x0000000000801002]              ( CPF_Const | CPF_Native | CPF_NoExport )
    // struct FScriptDelegate                             __ExternalCallback_OnDisplayNotification__Delegate;		// 0x0068 (0x000C) [0x0000000000400000]              ( CPF_NeedCtorLink )
    // struct FScriptDelegate                             __ExternalCallback_ClearNotifications__Delegate;  		// 0x0074 (0x000C) [0x0000000000400000]              ( CPF_NeedCtorLink )
    // struct FScriptDelegate                             __ExternalCallback_SetState__Delegate;            		// 0x0080 (0x000C) [0x0000000000400000]              ( CPF_NeedCtorLink )
    // struct FScriptDelegate                             __ExternalCallback_CloseEANetworking__Delegate;   		// 0x008C (0x000C) [0x0000000000400000]              ( CPF_NeedCtorLink )
    // struct FScriptDelegate                             __ExternalCallback_HasCerberusDLC__Delegate;      		// 0x0098 (0x000C) [0x0000000000400000]              ( CPF_NeedCtorLink )
    // struct FScriptDelegate                             __ExternalCallback_ShowStore__Delegate;           		// 0x00A4 (0x000C) [0x0000000000400000]              ( CPF_NeedCtorLink )
    // struct FName                                       HandlerId;                                        		// 0x00B0 (0x0008) [0x0000000000000000]
    // class USFXSFHandler_EANetworking*                  m_oGUI;
}

impl USFXOnlineComponentUI {
    define_method!(event_on_display_notification, 78599, info: FSFXOnlineMOTDInfo);
}

#[repr(C, packed(4))]
pub struct FSFXOnlineMOTDInfo {
    pub message: FString,
    pub title: FString,
    pub image: FString,
    pub tracking_id: ::std::os::raw::c_int,
    pub priority: ::std::os::raw::c_int,
    pub bw_ent_id: ::std::os::raw::c_int,
    pub offer_id: ::std::os::raw::c_int,
    pub ty: ::std::os::raw::c_uchar,
}
