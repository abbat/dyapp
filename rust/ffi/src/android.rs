//! Hands the app's `Context` to `ndk-context`: the libp2p DNS resolver reads the Android name
//! servers through it, and panics when nobody set it (ADR 0032).

use std::sync::Once;

use jni_sys::{jobject, JNIEnv, JavaVM, JNI_OK};

static INIT: Once = Once::new();

/// `DYApplication.nativeInit(context)`: stores the Java VM and a global reference to `context`
/// once per process; later calls do nothing.
///
/// # Safety
///
/// Only the JVM calls this, with the calling thread's `env` and a live `context`.
#[no_mangle]
pub unsafe extern "system" fn Java_com_dyapp_DYApplication_nativeInit(
    env: *mut JNIEnv,
    _application: jobject,
    context: jobject,
) {
    INIT.call_once(|| {
        // SAFETY: `env` is valid on this thread; the global reference keeps `context` alive for
        // the process, which ndk-context needs.
        unsafe {
            let table = &**env;
            let mut vm: *mut JavaVM = std::ptr::null_mut();
            if (table.v1_2.GetJavaVM)(env, &mut vm) != JNI_OK {
                return;
            }
            let context = (table.v1_2.NewGlobalRef)(env, context);
            ndk_context::initialize_android_context(vm.cast(), context.cast());
        }
    });
}
