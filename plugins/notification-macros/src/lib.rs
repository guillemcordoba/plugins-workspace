use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, ItemFn};

#[proc_macro_attribute]
pub fn modify_push_notification(_args: TokenStream, input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as ItemFn);
    let fn_name = input.sig.ident.clone();

    let expanded = quote! {
        #[cfg(target_os = "android")]
        tauri::wry::prelude::android_fn!(
            app_tauri,
            notification,
            PushNotificationsService,
            modifypushnotification,
            [jni::objects::JString<'local>],
            jni::objects::JString<'local>,
            [#fn_name]
        );

        // #[cfg(target_os = "android")]
        // unsafe fn setup_android_log() {
        //     let mut logpipe: [RawFd; 2] = Default::default();
        //     libc::pipe(logpipe.as_mut_ptr());
        //     libc::dup2(logpipe[1], libc::STDOUT_FILENO);
        //     libc::dup2(logpipe[1], libc::STDERR_FILENO);
        //     std::thread::spawn(move || {
        //         let tag = CStr::from_bytes_with_nul(b"RustStdoutStderr\0").unwrap();
        //         let file = File::from_raw_fd(logpipe[0]);
        //         let mut reader = BufReader::new(file);
        //         let mut buffer = String::new();
        //         loop {
        //             buffer.clear();
        //             if let Ok(len) = reader.read_line(&mut buffer) {
        //                 if len == 0 {
        //                     break;
        //                 } else if let Ok(msg) = CString::new(buffer.clone()) {
        //                     android_log(Level::Info, tag, &msg);
        //                 }
        //             }
        //         }
        //     });
        // }

        #[cfg(target_os = "android")]
        unsafe fn modifypushnotification<'local>(
            mut env: jni::JNIEnv<'local>,
            class: jni::objects::JClass<'local>,
            jnotification: jni::objects::JString<'local>,
            main: fn(tauri_plugin_notification::NotificationData) -> tauri_plugin_notification::NotificationData,
        ) -> jni::objects::JString<'local> {
            let notification: String = env
                .get_string(&jnotification)
                .expect("Couldn't get java string!")
                .into();

            let notification_data: tauri_plugin_notification::NotificationData = serde_json::from_str(notification.as_str()).expect("Can't convert notification");

            let modified_notification = main(notification_data);

            let jstring: jni::objects::JString = env.new_string(serde_json::to_string(&modified_notification).expect("Can't serialize NotificationData").clone()).expect("Coulnd't reserve new string");

            jstring
        }

        #[cfg(target_os = "ios")]
        #[repr(C)]
        pub struct RustByteSlice {
            pub bytes: *const u8,
            pub len: usize,
        }
        #[cfg(target_os = "ios")]
        impl RustByteSlice {
            fn as_str(&self) -> &str {
                unsafe {
                    // from_utf8_unchecked is sound because we checked in the constructor
                    std::str::from_utf8_unchecked(std::slice::from_raw_parts(self.bytes, self.len))
                }
            }
        }
        #[cfg(target_os = "ios")]
        impl<'a> From<&'a str> for RustByteSlice {
            fn from(s: &'a str) -> Self {
                RustByteSlice{
                    bytes: s.as_ptr(),
                    len: s.len() as usize,
                }
            }
        }
        #[cfg(target_os = "ios")]
        #[no_mangle]
        pub unsafe extern "C" fn notification_destroy(data: *mut RustByteSlice) {
            let _ = Box::from_raw(data);
        }
        #[cfg(target_os = "ios")]
        #[no_mangle]
        pub unsafe extern "C" fn notification_title(data: *const tauri_plugin_notification::NotificationData) -> RustByteSlice {
            let named_data = &*data;
            match &named_data.title {
                Some(b) => RustByteSlice::from(b.as_ref()),
                None => RustByteSlice::from("")
            }
        }
        #[cfg(target_os = "ios")]
        #[no_mangle]
        pub unsafe extern "C" fn notification_body(data: *const ::tauri_plugin_notification::NotificationData) -> RustByteSlice {
            let named_data = &*data;
            match &named_data.body {
                Some(b) => RustByteSlice::from(b.as_ref()),
                None => RustByteSlice::from("")
            }
        }
        #[cfg(target_os = "ios")]
        #[no_mangle]
        pub unsafe extern "C" fn modify_notification(notification_str: RustByteSlice) -> *mut tauri_plugin_notification::NotificationData {
            let notification: tauri_plugin_notification::NotificationData = serde_json::from_str(notification_str.as_str()).unwrap();

            let new_notification = #fn_name(notification);

           // let new_notification_str = serde_json::to_string(&new_notification).unwrap();
            //let s = &*new_notification_str;
            let boxed_data = Box::new(new_notification);
            Box::into_raw(boxed_data)
        }

        #input
    };

    TokenStream::from(expanded)
}
