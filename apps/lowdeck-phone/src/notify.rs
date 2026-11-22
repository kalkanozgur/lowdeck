//! Android foreground service + PC alarm bildirimi (ADR-0011). Masaüstünde no-op.
//! Java tarafı: `android/java/dev/kalkan/lowdeck/DeckService.java`.

/// Bildirim üreten alarm adları → bildirim kimliği (servisin kalıcı bildirimi 1).
/// Sıcaklık/kullanım alarmları burada yok: bant + perde yeterli (kullanıcı kararı 2026-11-22).
pub fn alert_id(name: &str) -> Option<i32> {
    match name {
        "uac" => Some(10),
        "firewall" => Some(11),
        "Claude.Input" => Some(12),
        _ => None,
    }
}

#[cfg(target_os = "android")]
mod imp {
    use jni::objects::{GlobalRef, JClass, JObject, JValue};
    use jni::JavaVM;
    use std::sync::OnceLock;

    struct Java {
        vm: JavaVM,
        class: GlobalRef,
        activity: GlobalRef,
    }
    static J: OnceLock<Java> = OnceLock::new();

    /// Servisi başlatır; sınıf Activity'nin ClassLoader'ıyla yüklenir (native thread'den
    /// `FindClass` uygulama sınıfını bulamaz). Hata: bildirim olmaz, uygulama çalışır.
    pub fn start(app: &slint::android::AndroidApp) -> jni::errors::Result<()> {
        // SAFETY: android-activity'nin verdiği geçerli JavaVM ve Activity işaretçileri; Activity
        // yaşam döngüsü boyunca geçerli, GlobalRef ile tutulur.
        let vm = unsafe { JavaVM::from_raw(app.vm_as_ptr().cast())? };
        let mut env = vm.attach_current_thread_permanently()?;
        let act = unsafe { JObject::from_raw(app.activity_as_ptr().cast()) };
        let r = (|| -> jni::errors::Result<_> {
            let loader = env
                .call_method(&act, "getClassLoader", "()Ljava/lang/ClassLoader;", &[])?
                .l()?;
            let name = env.new_string("dev.kalkan.lowdeck.DeckService")?;
            let cls = env
                .call_method(
                    &loader,
                    "loadClass",
                    "(Ljava/lang/String;)Ljava/lang/Class;",
                    &[JValue::Object(&name)],
                )?
                .l()?;
            env.call_static_method(
                <&JClass>::from(&cls),
                "start",
                "(Landroid/app/Activity;)V",
                &[JValue::Object(&act)],
            )?;
            Ok((env.new_global_ref(&cls)?, env.new_global_ref(&act)?))
        })();
        // Bekleyen Java istisnası bu thread'deki sonraki JNI çağrılarını (Slint dahil) bozar.
        if env.exception_check().unwrap_or(false) {
            let _ = env.exception_clear();
        }
        let (class, activity) = r?;
        let _ = J.set(Java {
            vm,
            class,
            activity,
        });
        Ok(())
    }

    fn call(f: impl FnOnce(&mut jni::JNIEnv, &JClass, &JObject) -> jni::errors::Result<()>) {
        let Some(j) = J.get() else { return };
        let r =
            j.vm.attach_current_thread_permanently()
                .and_then(|mut env| {
                    let r = f(
                        &mut env,
                        <&JClass>::from(j.class.as_obj()),
                        j.activity.as_obj(),
                    );
                    if env.exception_check().unwrap_or(false) {
                        let _ = env.exception_clear();
                    }
                    r
                });
        if let Err(e) = r {
            eprintln!("notify: {e}");
        }
    }

    pub fn alert(id: i32, title: &str) {
        call(|env, cls, act| {
            let t = env.new_string(title)?;
            env.call_static_method(
                cls,
                "alert",
                "(Landroid/content/Context;ILjava/lang/String;)V",
                &[JValue::Object(act), JValue::Int(id), JValue::Object(&t)],
            )?;
            Ok(())
        })
    }

    pub fn cancel(id: i32) {
        call(|env, cls, act| {
            env.call_static_method(
                cls,
                "cancel",
                "(Landroid/content/Context;I)V",
                &[JValue::Object(act), JValue::Int(id)],
            )?;
            Ok(())
        })
    }
}

#[cfg(target_os = "android")]
pub use imp::{alert, cancel, start};

#[cfg(not(target_os = "android"))]
pub fn alert(_id: i32, _title: &str) {}
#[cfg(not(target_os = "android"))]
pub fn cancel(_id: i32) {}

#[cfg(test)]
mod tests {
    #[test]
    fn only_security_and_claude_notify() {
        use super::alert_id;
        assert_eq!(alert_id("uac"), Some(10));
        assert_eq!(alert_id("firewall"), Some(11));
        assert_eq!(alert_id("Claude.Input"), Some(12));
        assert_eq!(alert_id("Temp.CPU"), None);
        assert_eq!(alert_id("cpu_high"), None);
    }
}
