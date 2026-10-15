//! Adds a notification service extension target to the app's Xcode project,
//! so the app's `#[receive_push_notification]` handler runs on every push.
//!
//! Runs from the plugin's build script, which runs while xcodebuild already
//! builds the app: the target joins the build from the next one.

use std::{
    fs,
    hash::{Hash, Hasher},
    path::Path,
};

const TARGET: &str = "NotificationService";
const SERVICE_EXTENSION_POINT: &str = "com.apple.usernotifications.service";

pub fn ensure(project_dir: &Path, app_name: &str) -> Result<(), String> {
    let pbxproj_path = project_dir
        .join(format!("{app_name}.xcodeproj"))
        .join("project.pbxproj");
    let pbxproj = fs::read_to_string(&pbxproj_path).map_err(|e| e.to_string())?;
    if has_service_extension(&pbxproj, project_dir) {
        return sync_versions(project_dir, app_name);
    }

    let app = AppTarget::find(&pbxproj).ok_or("no application target in project.pbxproj")?;
    write_extension_files(project_dir, app_name, &app)?;
    let pbxproj = add_target(pbxproj, &app);
    fs::write(&pbxproj_path, pbxproj).map_err(|e| e.to_string())?;
    sync_versions(project_dir, app_name)?;
    println!(
        "cargo:warning=Added the {TARGET} push notification extension to {}; it is built from the next build",
        pbxproj_path.display()
    );
    Ok(())
}

/// iOS refuses an extension whose versions differ from its app's, and Tauri
/// only writes them into the app's Info.plist.
fn sync_versions(project_dir: &Path, app_name: &str) -> Result<(), String> {
    let app_info_plist = project_dir
        .join(format!("{app_name}_iOS"))
        .join("Info.plist");
    println!("cargo:rerun-if-changed={}", app_info_plist.display());
    let extension_info_plist = project_dir.join(TARGET).join("Info.plist");
    let (Ok(app), Ok(mut extension)) = (
        plist::Value::from_file(&app_info_plist),
        plist::Value::from_file(&extension_info_plist),
    ) else {
        return Ok(());
    };
    let (Some(app), Some(extension_dict)) = (app.as_dictionary(), extension.as_dictionary_mut())
    else {
        return Ok(());
    };
    let mut changed = false;
    for key in ["CFBundleShortVersionString", "CFBundleVersion"] {
        if let Some(version) = app.get(key) {
            if extension_dict.get(key) != Some(version) {
                extension_dict.insert(key.to_string(), version.clone());
                changed = true;
            }
        }
    }
    if changed {
        extension
            .to_file_xml(&extension_info_plist)
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Whether some target already is a notification service extension, ours or
/// one the app wrote itself.
fn has_service_extension(pbxproj: &str, project_dir: &Path) -> bool {
    setting_values(pbxproj, "INFOPLIST_FILE").any(|info_plist| {
        fs::read_to_string(project_dir.join(info_plist.trim_matches('"')))
            .is_ok_and(|plist| plist.contains(SERVICE_EXTENSION_POINT))
    })
}

struct AppTarget {
    id: String,
    bundle_id: String,
    team: Option<String>,
    deployment_target: Option<String>,
    /// libapp.a is only built for the app's architectures.
    arch_settings: String,
    rust_build_script: Option<String>,
}

impl AppTarget {
    fn find(pbxproj: &str) -> Option<Self> {
        let (id, block) = objects(pbxproj, "PBXNativeTarget")
            .find(|(_, block)| block.contains("\"com.apple.product-type.application\""))?;
        let config_list = field(block, "buildConfigurationList")?;
        let configs = list(object(pbxproj, &config_list)?, "buildConfigurations");
        let settings = |key| {
            configs
                .iter()
                .find_map(|config| object(pbxproj, config).and_then(|c| field(c, key)))
        };
        let rust_build_script = list(block, "buildPhases").into_iter().find_map(|phase| {
            let phase = object(pbxproj, &phase)?;
            phase
                .contains("tauri ios xcode-script")
                .then(|| field(phase, "shellScript"))?
        });
        let arch_settings = configs
            .first()
            .and_then(|config| object(pbxproj, config))
            .map(arch_settings)
            .unwrap_or_default();
        Some(Self {
            id,
            arch_settings,
            bundle_id: settings("PRODUCT_BUNDLE_IDENTIFIER")?,
            team: settings("DEVELOPMENT_TEAM"),
            deployment_target: settings("IPHONEOS_DEPLOYMENT_TARGET"),
            rust_build_script,
        })
    }
}

fn write_extension_files(
    project_dir: &Path,
    app_name: &str,
    app: &AppTarget,
) -> Result<(), String> {
    let dir = project_dir.join(TARGET);
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let app_group = format!("group.{}", app.bundle_id.trim_matches('"'));
    let files = [
        (
            "Info.plist".to_string(),
            plist_xml(&format!(
                "\t<key>CFBundleDisplayName</key>\n\t<string>{TARGET}</string>\n\t<key>CFBundleExecutable</key>\n\t<string>$(EXECUTABLE_NAME)</string>\n\t<key>CFBundleIdentifier</key>\n\t<string>$(PRODUCT_BUNDLE_IDENTIFIER)</string>\n\t<key>CFBundleInfoDictionaryVersion</key>\n\t<string>6.0</string>\n\t<key>CFBundleName</key>\n\t<string>$(PRODUCT_NAME)</string>\n\t<key>CFBundlePackageType</key>\n\t<string>$(PRODUCT_BUNDLE_PACKAGE_TYPE)</string>\n\t<key>NSExtension</key>\n\t<dict>\n\t\t<key>NSExtensionPointIdentifier</key>\n\t\t<string>{SERVICE_EXTENSION_POINT}</string>\n\t\t<key>NSExtensionPrincipalClass</key>\n\t\t<string>TauriNotificationService</string>\n\t</dict>\n"
            )),
        ),
        (
            format!("{TARGET}.entitlements"),
            plist_xml(&app_group_entitlement(&app_group)),
        ),
        (
            "TauriNotificationService.c".to_string(),
            "// Links tauri-plugin-notification's TauriNotificationService, the principal\n// class in Info.plist, out of libapp.a: iOS only looks it up by name.\nextern void tauri_notification_service(void);\nvoid (*tauri_notification_service_link)(void) = tauri_notification_service;\n".to_string(),
        ),
    ];
    for (name, contents) in files {
        let path = dir.join(name);
        if !path.exists() {
            fs::write(&path, contents).map_err(|e| e.to_string())?;
        }
    }
    add_app_group_to_app(project_dir, app_name, &app_group)
}

/// The extension reads the app's data through the App Group container they
/// share.
fn add_app_group_to_app(project_dir: &Path, app_name: &str, app_group: &str) -> Result<(), String> {
    let path = project_dir
        .join(format!("{app_name}_iOS"))
        .join(format!("{app_name}_iOS.entitlements"));
    let entitlements = fs::read_to_string(&path)
        .unwrap_or_else(|_| plist_xml(""))
        .replace("<dict/>", "<dict>\n</dict>");
    if entitlements.contains(app_group) {
        return Ok(());
    }
    let entitlements = entitlements.replacen(
        "<dict>",
        &format!("<dict>\n{}", app_group_entitlement(app_group).trim_end()),
        1,
    );
    fs::write(&path, entitlements).map_err(|e| e.to_string())
}

fn app_group_entitlement(app_group: &str) -> String {
    format!(
        "\t<key>com.apple.security.application-groups</key>\n\t<array>\n\t\t<string>{app_group}</string>\n\t</array>\n"
    )
}

fn plist_xml(dict: &str) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n<plist version=\"1.0\">\n<dict>\n{dict}</dict>\n</plist>\n"
    )
}

fn add_target(mut pbxproj: String, app: &AppTarget) -> String {
    let id = |name: &str| object_id(&app.bundle_id, name);
    let (target, product, embed_file, embed_phase) = (
        id("target"),
        id("product"),
        id("embed-file"),
        id("embed-phase"),
    );
    let (proxy, dependency, config_list) = (id("proxy"), id("dependency"), id("config-list"));
    let (group, stub, stub_build) = (id("group"), id("stub"), id("stub-build"));
    let (rust_phase, sources_phase, frameworks_phase) = (
        id("rust-phase"),
        id("sources-phase"),
        id("frameworks-phase"),
    );
    let root_object = field(&pbxproj, "rootObject").unwrap_or_default();

    let configs: Vec<(String, &str)> = ["debug", "release"]
        .into_iter()
        .map(|name| (id(&format!("config-{name}")), name))
        .collect();
    for (config, name) in &configs {
        insert_object(
            &mut pbxproj,
            "XCBuildConfiguration",
            &format!(
                "\t\t{config} /* {name} */ = {{\n\t\t\tisa = XCBuildConfiguration;\n\t\t\tbuildSettings = {{\n{}\t\t\t}};\n\t\t\tname = {name};\n\t\t}};\n",
                build_settings(app)
            ),
        );
    }
    insert_object(
        &mut pbxproj,
        "XCConfigurationList",
        &format!(
            "\t\t{config_list} /* Build configuration list for PBXNativeTarget \"{TARGET}\" */ = {{\n\t\t\tisa = XCConfigurationList;\n\t\t\tbuildConfigurations = (\n{}\t\t\t);\n\t\t\tdefaultConfigurationIsVisible = 0;\n\t\t\tdefaultConfigurationName = release;\n\t\t}};\n",
            configs
                .iter()
                .map(|(config, name)| format!("\t\t\t\t{config} /* {name} */,\n"))
                .collect::<String>()
        ),
    );
    insert_object(
        &mut pbxproj,
        "PBXFileReference",
        &format!(
            "\t\t{product} /* {TARGET}.appex */ = {{isa = PBXFileReference; explicitFileType = \"wrapper.app-extension\"; includeInIndex = 0; path = {TARGET}.appex; sourceTree = BUILT_PRODUCTS_DIR; }};\n\t\t{stub} /* TauriNotificationService.c */ = {{isa = PBXFileReference; lastKnownFileType = sourcecode.c.c; path = TauriNotificationService.c; sourceTree = \"<group>\"; }};\n"
        ),
    );
    insert_object(
        &mut pbxproj,
        "PBXBuildFile",
        &format!(
            "\t\t{embed_file} /* {TARGET}.appex in Embed Foundation Extensions */ = {{isa = PBXBuildFile; fileRef = {product} /* {TARGET}.appex */; settings = {{ATTRIBUTES = (RemoveHeadersOnCopy, ); }}; }};\n\t\t{stub_build} /* TauriNotificationService.c in Sources */ = {{isa = PBXBuildFile; fileRef = {stub} /* TauriNotificationService.c */; }};\n"
        ),
    );
    insert_object(
        &mut pbxproj,
        "PBXGroup",
        &format!(
            "\t\t{group} /* {TARGET} */ = {{\n\t\t\tisa = PBXGroup;\n\t\t\tchildren = (\n\t\t\t\t{stub} /* TauriNotificationService.c */,\n\t\t\t);\n\t\t\tpath = {TARGET};\n\t\t\tsourceTree = \"<group>\";\n\t\t}};\n"
        ),
    );
    insert_object(
        &mut pbxproj,
        "PBXSourcesBuildPhase",
        &format!(
            "\t\t{sources_phase} /* Sources */ = {{\n\t\t\tisa = PBXSourcesBuildPhase;\n\t\t\tbuildActionMask = 2147483647;\n\t\t\tfiles = (\n\t\t\t\t{stub_build} /* TauriNotificationService.c in Sources */,\n\t\t\t);\n\t\t\trunOnlyForDeploymentPostprocessing = 0;\n\t\t}};\n"
        ),
    );
    insert_object(
        &mut pbxproj,
        "PBXFrameworksBuildPhase",
        &format!(
            "\t\t{frameworks_phase} /* Frameworks */ = {{\n\t\t\tisa = PBXFrameworksBuildPhase;\n\t\t\tbuildActionMask = 2147483647;\n\t\t\tfiles = (\n\t\t\t);\n\t\t\trunOnlyForDeploymentPostprocessing = 0;\n\t\t}};\n"
        ),
    );
    insert_object(
        &mut pbxproj,
        "PBXCopyFilesBuildPhase",
        &format!(
            "\t\t{embed_phase} /* Embed Foundation Extensions */ = {{\n\t\t\tisa = PBXCopyFilesBuildPhase;\n\t\t\tbuildActionMask = 2147483647;\n\t\t\tdstPath = \"\";\n\t\t\tdstSubfolderSpec = 13;\n\t\t\tfiles = (\n\t\t\t\t{embed_file} /* {TARGET}.appex in Embed Foundation Extensions */,\n\t\t\t);\n\t\t\tname = \"Embed Foundation Extensions\";\n\t\t\trunOnlyForDeploymentPostprocessing = 0;\n\t\t}};\n"
        ),
    );
    insert_object(
        &mut pbxproj,
        "PBXContainerItemProxy",
        &format!(
            "\t\t{proxy} /* PBXContainerItemProxy */ = {{\n\t\t\tisa = PBXContainerItemProxy;\n\t\t\tcontainerPortal = {root_object} /* Project object */;\n\t\t\tproxyType = 1;\n\t\t\tremoteGlobalIDString = {target};\n\t\t\tremoteInfo = {TARGET};\n\t\t}};\n"
        ),
    );
    insert_object(
        &mut pbxproj,
        "PBXTargetDependency",
        &format!(
            "\t\t{dependency} /* PBXTargetDependency */ = {{\n\t\t\tisa = PBXTargetDependency;\n\t\t\ttarget = {target} /* {TARGET} */;\n\t\t\ttargetProxy = {proxy} /* PBXContainerItemProxy */;\n\t\t}};\n"
        ),
    );
    // The extension links libapp.a and builds before the app embeds it, so it
    // builds the Rust code itself. No output paths: the app's phase owns them.
    let mut phases = format!(
        "\t\t\t\t{sources_phase} /* Sources */,\n\t\t\t\t{frameworks_phase} /* Frameworks */,\n"
    );
    if let Some(script) = &app.rust_build_script {
        insert_object(
            &mut pbxproj,
            "PBXShellScriptBuildPhase",
            &format!(
                "\t\t{rust_phase} /* Build Rust Code */ = {{\n\t\t\tisa = PBXShellScriptBuildPhase;\n\t\t\talwaysOutOfDate = 1;\n\t\t\tbuildActionMask = 2147483647;\n\t\t\tfiles = (\n\t\t\t);\n\t\t\tinputPaths = (\n\t\t\t);\n\t\t\tname = \"Build Rust Code\";\n\t\t\toutputPaths = (\n\t\t\t);\n\t\t\trunOnlyForDeploymentPostprocessing = 0;\n\t\t\tshellPath = /bin/sh;\n\t\t\tshellScript = {script};\n\t\t}};\n"
            ),
        );
        phases = format!("\t\t\t\t{rust_phase} /* Build Rust Code */,\n{phases}");
    }
    insert_object(
        &mut pbxproj,
        "PBXNativeTarget",
        &format!(
            "\t\t{target} /* {TARGET} */ = {{\n\t\t\tisa = PBXNativeTarget;\n\t\t\tbuildConfigurationList = {config_list} /* Build configuration list for PBXNativeTarget \"{TARGET}\" */;\n\t\t\tbuildPhases = (\n{phases}\t\t\t);\n\t\t\tbuildRules = (\n\t\t\t);\n\t\t\tdependencies = (\n\t\t\t);\n\t\t\tname = {TARGET};\n\t\t\tproductName = {TARGET};\n\t\t\tproductReference = {product} /* {TARGET}.appex */;\n\t\t\tproductType = \"com.apple.product-type.app-extension\";\n\t\t}};\n"
        ),
    );

    append_to_list(
        &mut pbxproj,
        &app.id,
        "buildPhases",
        &format!("{embed_phase} /* Embed Foundation Extensions */"),
    );
    append_to_list(
        &mut pbxproj,
        &app.id,
        "dependencies",
        &format!("{dependency} /* PBXTargetDependency */"),
    );
    append_to_list(
        &mut pbxproj,
        &root_object,
        "targets",
        &format!("{target} /* {TARGET} */"),
    );
    if let Some(products) = group_named(&pbxproj, "Products") {
        append_to_list(
            &mut pbxproj,
            &products,
            "children",
            &format!("{product} /* {TARGET}.appex */"),
        );
    }
    if let Some(main_group) = object(&pbxproj, &root_object).and_then(|p| field(p, "mainGroup")) {
        append_to_list(
            &mut pbxproj,
            &main_group,
            "children",
            &format!("{group} /* {TARGET} */"),
        );
    }
    pbxproj
}

fn build_settings(app: &AppTarget) -> String {
    let bundle_id = format!("{}.{TARGET}", app.bundle_id.trim_matches('"'));
    let mut settings = vec![
        ("APPLICATION_EXTENSION_API_ONLY", "YES".to_string()),
        ("CODE_SIGN_ENTITLEMENTS", format!("{TARGET}/{TARGET}.entitlements")),
        ("CODE_SIGN_STYLE", "Automatic".to_string()),
        // Not generated: Xcode would rewrite the versions sync_versions copies.
        ("INFOPLIST_FILE", format!("{TARGET}/Info.plist")),
        (
            "LD_RUNPATH_SEARCH_PATHS",
            "(\"$(inherited)\", \"@executable_path/Frameworks\", \"@executable_path/../../Frameworks\", )".to_string(),
        ),
        ("\"LIBRARY_SEARCH_PATHS[arch=arm64]\"", library_search_paths("arm64")),
        ("\"LIBRARY_SEARCH_PATHS[arch=x86_64]\"", library_search_paths("x86_64")),
        // Not libapp.a's path: Xcode would then make this target depend on the
        // app's Rust build phase, while the app depends on this target.
        ("OTHER_LDFLAGS", "(\"$(inherited)\", \"-lapp\", )".to_string()),
        ("PRODUCT_BUNDLE_IDENTIFIER", format!("\"{bundle_id}\"")),
        ("PRODUCT_NAME", "\"$(TARGET_NAME)\"".to_string()),
        ("SDKROOT", "iphoneos".to_string()),
        ("SKIP_INSTALL", "YES".to_string()),
        ("TARGETED_DEVICE_FAMILY", "\"1,2\"".to_string()),
    ];
    if let Some(team) = &app.team {
        settings.push(("DEVELOPMENT_TEAM", team.clone()));
    }
    if let Some(target) = &app.deployment_target {
        settings.push(("IPHONEOS_DEPLOYMENT_TARGET", target.clone()));
    }
    let settings: String = settings
        .into_iter()
        .map(|(key, value)| format!("\t\t\t\t{key} = {value};\n"))
        .collect();
    settings + &app.arch_settings
}

fn library_search_paths(arch: &str) -> String {
    format!(
        "(\"$(inherited)\", \"$(PROJECT_DIR)/Externals/{arch}/$(CONFIGURATION)\", \"$(SDKROOT)/usr/lib/swift\", \"$(TOOLCHAIN_DIR)/usr/lib/swift/$(PLATFORM_NAME)\", \"$(TOOLCHAIN_DIR)/usr/lib/swift-5.0/$(PLATFORM_NAME)\", )"
    )
}

/// The `ARCHS`, `VALID_ARCHS` and `EXCLUDED_ARCHS` settings of a build
/// configuration, verbatim.
fn arch_settings(config: &str) -> String {
    let mut settings = String::new();
    let mut lines = config.lines();
    while let Some(line) = lines.next() {
        let key = line.trim_start().trim_start_matches('"');
        if !["ARCHS", "VALID_ARCHS", "EXCLUDED_ARCHS"]
            .iter()
            .any(|arch_key| {
                key.starts_with(&format!("{arch_key} ")) || key.starts_with(&format!("{arch_key}["))
            })
        {
            continue;
        }
        settings.push_str(line);
        settings.push('\n');
        if line.trim_end().ends_with('(') {
            for line in lines.by_ref() {
                settings.push_str(line);
                settings.push('\n');
                if line.trim() == ");" {
                    break;
                }
            }
        }
    }
    settings
}

/// Stable across builds, so a target added twice would collide rather than
/// duplicate.
fn object_id(seed: &str, name: &str) -> String {
    let mut hasher = Fnv(0xcbf29ce484222325);
    (seed, name).hash(&mut hasher);
    let a = hasher.finish();
    "tauri-plugin-notification".hash(&mut hasher);
    format!("{:016X}{:08X}", a, hasher.finish() as u32)
}

struct Fnv(u64);

impl Hasher for Fnv {
    fn finish(&self) -> u64 {
        self.0
    }
    fn write(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.0 = (self.0 ^ *byte as u64).wrapping_mul(0x100000001b3);
        }
    }
}

fn insert_object(pbxproj: &mut String, isa: &str, object: &str) {
    let end = format!("/* End {isa} section */");
    match pbxproj.find(&end) {
        Some(at) => pbxproj.insert_str(at, object),
        None => {
            let at = pbxproj
                .find("/* Begin ")
                .expect("no sections in project.pbxproj");
            pbxproj.insert_str(at, &format!("/* Begin {isa} section */\n{object}{end}\n\n"));
        }
    }
}

fn append_to_list(pbxproj: &mut String, object_id: &str, key: &str, entry: &str) {
    let Some(start) = object_start(pbxproj, object_id) else {
        return;
    };
    let Some(list) = pbxproj[start..].find(&format!("\t{key} = (")) else {
        return;
    };
    let close = pbxproj[start + list..].find(");").unwrap() + start + list;
    let line_start = pbxproj[..close].rfind('\n').unwrap() + 1;
    pbxproj.insert_str(line_start, &format!("\t\t\t\t{entry},\n"));
}

fn objects<'a>(pbxproj: &'a str, isa: &str) -> impl Iterator<Item = (String, &'a str)> {
    let section = pbxproj
        .split_once(&format!("/* Begin {isa} section */"))
        .and_then(|(_, rest)| rest.split_once(&format!("/* End {isa} section */")))
        .map_or("", |(section, _)| section);
    section.split("\n\t\t};").filter_map(|block| {
        let block = block.trim_start_matches(['\n', '\t']);
        let id = block.split_whitespace().next()?.to_string();
        Some((id, block))
    })
}

fn object_start(pbxproj: &str, id: &str) -> Option<usize> {
    pbxproj.find(&format!("\n\t\t{id} ")).map(|at| at + 1)
}

fn object<'a>(pbxproj: &'a str, id: &str) -> Option<&'a str> {
    let start = object_start(pbxproj, id)?;
    let end = pbxproj[start..].find("\n\t\t};")? + start;
    Some(&pbxproj[start..end])
}

fn group_named(pbxproj: &str, name: &str) -> Option<String> {
    objects(pbxproj, "PBXGroup")
        .find(|(_, block)| block.contains(&format!("\tname = {name};")))
        .map(|(id, _)| id)
}

fn field(block: &str, key: &str) -> Option<String> {
    setting_values(block, key).next().map(str::to_string)
}

fn setting_values<'a>(block: &'a str, key: &'a str) -> impl Iterator<Item = &'a str> + 'a {
    let pattern = format!("\t{key} = ");
    let starts: Vec<usize> = block
        .match_indices(&pattern)
        .map(|(at, _)| at + pattern.len())
        .collect();
    starts.into_iter().filter_map(move |at| {
        let value = &block[at..];
        let end = if value.starts_with('"') {
            closing_quote(value)? + 1
        } else {
            value.find(';')?
        };
        Some(value[..end].split(" /*").next().unwrap_or("").trim())
    })
}

/// The index of the quote closing the string `value` starts with.
fn closing_quote(value: &str) -> Option<usize> {
    let mut escaped = false;
    for (at, c) in value.char_indices().skip(1) {
        match c {
            '\\' if !escaped => escaped = true,
            '"' if !escaped => return Some(at),
            _ => escaped = false,
        }
    }
    None
}

fn list(block: &str, key: &str) -> Vec<String> {
    let Some(start) = block.find(&format!("\t{key} = (")) else {
        return vec![];
    };
    let rest = &block[start..];
    let rest = &rest[rest.find('(').unwrap() + 1..rest.find(");").unwrap_or(rest.len())];
    rest.split(',')
        .filter_map(|entry| entry.split_whitespace().next())
        .map(str::to_string)
        .collect()
}
