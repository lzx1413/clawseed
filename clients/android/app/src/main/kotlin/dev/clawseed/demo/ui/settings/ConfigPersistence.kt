package dev.clawseed.demo.ui.settings

import java.io.File
import java.nio.file.Files
import java.nio.file.StandardCopyOption
import org.tomlj.Toml

internal object ConfigPersistence {
    suspend fun save(
        toml: String,
        gatewaySave: (suspend (String) -> Result<Unit>)?,
        localFile: () -> File,
    ) {
        if (gatewaySave != null) {
            // The gateway validates the schema and restores masked credentials.
            gatewaySave(toml).getOrThrow()
            return
        }
        val parsed = Toml.parse(toml)
        require(!parsed.hasErrors()) { parsed.errors().joinToString("\n") }
        val target = localFile().toPath()
        val temporary = Files.createTempFile(target.parent, "config-", ".toml")
        try {
            Files.write(temporary, toml.toByteArray(Charsets.UTF_8))
            Files.move(temporary, target, StandardCopyOption.ATOMIC_MOVE, StandardCopyOption.REPLACE_EXISTING)
        } finally {
            Files.deleteIfExists(temporary)
        }
    }
}
