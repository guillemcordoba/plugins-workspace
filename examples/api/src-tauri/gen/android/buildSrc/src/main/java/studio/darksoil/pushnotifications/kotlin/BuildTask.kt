import java.io.File
import org.apache.tools.ant.taskdefs.condition.Os
import org.gradle.api.DefaultTask
import org.gradle.api.GradleException
import org.gradle.api.logging.LogLevel
import org.gradle.api.tasks.Input
import org.gradle.api.tasks.TaskAction

open class BuildTask : DefaultTask() {
    @Input
    var rootDirRel: String? = null
    @Input
    var target: String? = null
    @Input
    var release: Boolean? = null

    @TaskAction
    fun assemble() {
<<<<<<< HEAD:examples/api/src-tauri/gen/android/buildSrc/src/main/java/com/tauri/api/kotlin/BuildTask.kt
        val executable = """pnpm""";
=======
        val executable = """npm""";
>>>>>>> push-notifications:examples/api/src-tauri/gen/android/buildSrc/src/main/java/studio/darksoil/pushnotifications/kotlin/BuildTask.kt
        try {
            runTauriCli(executable)
        } catch (e: Exception) {
            if (Os.isFamily(Os.FAMILY_WINDOWS)) {
                runTauriCli("$executable.cmd")
            } else {
                throw e;
            }
        }
    }

    fun runTauriCli(executable: String) {
        val rootDirRel = rootDirRel ?: throw GradleException("rootDirRel cannot be null")
        val target = target ?: throw GradleException("target cannot be null")
        val release = release ?: throw GradleException("release cannot be null")
<<<<<<< HEAD:examples/api/src-tauri/gen/android/buildSrc/src/main/java/com/tauri/api/kotlin/BuildTask.kt
        val args = listOf("tauri", "android", "android-studio-script");
=======
        val args = listOf("run", "tauri", "--", "android", "android-studio-script");
>>>>>>> push-notifications:examples/api/src-tauri/gen/android/buildSrc/src/main/java/studio/darksoil/pushnotifications/kotlin/BuildTask.kt

        project.exec {
            workingDir(File(project.projectDir, rootDirRel))
            executable(executable)
            args(args)
            if (project.logger.isEnabled(LogLevel.DEBUG)) {
                args("-vv")
            } else if (project.logger.isEnabled(LogLevel.INFO)) {
                args("-v")
            }
            if (release) {
                args("--release")
            }
            args(listOf("--target", target))
        }.assertNormalExitValue()
    }
}