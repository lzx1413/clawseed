package dev.clawseed.sdk.embedded

import org.bouncycastle.crypto.params.Ed25519PrivateKeyParameters
import org.bouncycastle.crypto.signers.Ed25519Signer
import org.json.JSONObject
import java.util.Base64
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import java.nio.charset.StandardCharsets
import java.security.SecureRandom

class RuntimeManifestVerifierTest {
    @Test
    fun verifiesCanonicalManifestWithoutSignatureField() {
        val key = Ed25519PrivateKeyParameters(SecureRandom())
        val payload = "{\"abi\":\"arm64-v8a\",\"runtime_id\":\"fixture-arm64\",\"runtime_version\":\"fixture\",\"schema_version\":1}"
        val signature = sign(payload, key)

        assertTrue(
            RuntimeManifestVerifier.verifyPayload(
                payload.toByteArray(StandardCharsets.UTF_8),
                signature,
                key.generatePublicKey().encoded,
            ),
        )
    }

    @Test
    fun rejectsChangedManifest() {
        val key = Ed25519PrivateKeyParameters(SecureRandom())
        val signature = sign(
            "{\"abi\":\"arm64-v8a\",\"runtime_id\":\"fixture-arm64\",\"runtime_version\":\"fixture\",\"schema_version\":1}",
            key,
        )
        val tampered = "{\"abi\":\"arm64-v8a\",\"runtime_id\":\"fixture-arm64\",\"runtime_version\":\"tampered\",\"schema_version\":1}"

        assertFalse(
            RuntimeManifestVerifier.verifyPayload(
                tampered.toByteArray(StandardCharsets.UTF_8),
                signature,
                key.generatePublicKey().encoded,
            ),
        )
    }

    @Test
    fun verifiesManifestCanonicalizationAndRejectsChangedFields() {
        val key = Ed25519PrivateKeyParameters(SecureRandom())
        val manifest = JSONObject("""{"schema_version":1,"abi":"arm64-v8a","components":{"python":"3.12","node":"24"}}""")
        val canonical = RuntimeManifestVerifier.canonicalPayload(manifest)
        assertEquals("""{"abi":"arm64-v8a", "components":{"node":"24", "python":"3.12"}, "schema_version":1}""", canonical)
        manifest.put("signature", Base64.getEncoder().encodeToString(sign(canonical, key)))
        val publicKey = Base64.getEncoder().encodeToString(key.generatePublicKey().encoded)
        assertTrue(RuntimeManifestVerifier.verify(manifest, publicKey))
        manifest.getJSONObject("components").put("node", "other")
        assertFalse(RuntimeManifestVerifier.verify(manifest, publicKey))
    }

    private fun sign(payload: String, key: Ed25519PrivateKeyParameters): ByteArray {
        val signer = Ed25519Signer()
        signer.init(true, key)
        val bytes = payload.toByteArray(StandardCharsets.UTF_8)
        signer.update(bytes, 0, bytes.size)
        return signer.generateSignature()
    }
}
