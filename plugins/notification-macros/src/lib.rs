use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, ItemFn};

#[proc_macro_attribute]
pub fn receive_push_notification(_args: TokenStream, input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as ItemFn);
    let fn_name = input.sig.ident.clone();

    let expanded = quote! {
        #[cfg(target_os = "android")]
        tauri::wry::prelude::android_fn!(
            app_tauri,
            notification,
            PushNotificationsService,
            receivepushnotification,
            [jni::objects::JString<'local>, jni::objects::JString<'local>],
            jni::objects::JString<'local>,
            [#fn_name]
        );

        #[cfg(target_os = "android")]
        unsafe fn receivepushnotification<'local>(
            mut env: jni::JNIEnv<'local>,
            _class: jni::objects::JClass<'local>,
            jnotification: jni::objects::JString<'local>,
            jdata_dir: jni::objects::JString<'local>,
            main: fn(tauri_plugin_notification::NotificationData, tauri_plugin_notification::ReceivePushNotificationContext) -> Option<tauri_plugin_notification::NotificationData>,
        ) -> jni::objects::JString<'local> {

            let notification: String = env
                .get_string(&jnotification)
                .expect("Couldn't get java string!")
                .into();

            let data_dir: String = env
                .get_string(&jdata_dir)
                .expect("Couldn't get java string!")
                .into();

            let data_dir = std::path::PathBuf::from(data_dir.as_str());
            let context = tauri_plugin_notification::ReceivePushNotificationContext {
                data_dir
            };

            let notification_data: tauri_plugin_notification::NotificationData = serde_json::from_str(notification.as_str()).expect("Can't convert notification");
            let received_notification = match main(notification_data, context) {
                Some(n) => n,
                None => tauri_plugin_notification::NotificationData::default()
            };

            let jstring: jni::objects::JString = env.new_string(serde_json::to_string(&received_notification).expect("Can't serialize NotificationData").clone()).expect("Coulnd't reserve new string");

            jstring
        }

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

        #[cfg(target_os = "ios")]
        #[no_mangle]
        pub unsafe extern "C" fn tauri_plugin_notification_receive_push(
            notification: *const ::std::ffi::c_char,
            data_dir: *const ::std::ffi::c_char,
        ) -> *mut ::std::ffi::c_char {
            tauri_plugin_notification::ios_push_extension::receive(notification, data_dir, #fn_name)
        }

        #[cfg(target_os = "ios")]
        #[no_mangle]
        pub unsafe extern "C" fn tauri_plugin_notification_free_push(json: *mut ::std::ffi::c_char) {
            tauri_plugin_notification::ios_push_extension::free(json)
        }

        #input
    };

    TokenStream::from(expanded)
}
