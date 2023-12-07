use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, ItemFn};

#[proc_macro_attribute]
pub fn fetch_pending_notifications(_args: TokenStream, input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as ItemFn);
    let fn_name = input.sig.ident.clone();

    let expanded = quote! {
        #[cfg(target_os = "android")]
        tauri::wry::application::android_fn!(
            app_tauri,
            notification,
            PushNotificationsService,
            fetchpendingnotifications,
            [jni::objects::JObject<'local>],
            jni::objects::JObjectArray<'local>,
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
        unsafe fn fetchpendingnotifications<'local>(
            mut env: jni::JNIEnv<'local>,
            class: jni::objects::JClass<'local>,
            jobject: jni::objects::JObject<'local>,
            main: fn() -> Vec<tauri_plugin_notification::NotificationData>,
        ) -> jni::objects::JObjectArray<'local> {
            // setup_android_log();

            let pending_notifications = main();

            let jstrings: Vec<jni::objects::JString> = pending_notifications
                .into_iter()
                .filter_map(|s| serde_json::to_value(s).ok())
                .filter_map(|s| serde_json::to_string(&s).ok())
                .map(|s| env.new_string(s.clone())) // Convert to JString (maybe)
                .filter_map(Result::ok)
                .collect();

            let initial_value = env.new_string("").unwrap();
            let result = env
                .new_object_array(jstrings.len() as i32, "java/lang/String", initial_value)
                .unwrap();
            let mut i = 0;
            for argument in jstrings {
                // let value = env.new_string(argument);
                let _ = env.set_object_array_element(&result, i, argument);
                i = i + 1;
            }

            result
        }

        #input
    };

    TokenStream::from(expanded)
}
