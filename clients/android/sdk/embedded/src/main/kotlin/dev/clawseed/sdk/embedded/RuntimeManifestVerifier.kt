package dev.clawseed.sdk.embedded

import org.bouncycastle.crypto.params.Ed25519PublicKeyParameters
import org.bouncycastle.crypto.signers.Ed25519Signer
import org.json.JSONArray
import org.json.JSONObject
import java.nio.charset.StandardCharsets
import java.util.Base64

/** Verifies the canonical manifest payload without relying on an Android API-level provider. */
internal object RuntimeManifestVerifier {
    private const val SIGNATURE_FIELD = "signature"

    fun verify(manifest: JSONObject, publicKeyBase64: String): Boolean {
        val publicKey = runCatching { decodePublicKey(publicKeyBase64) }.getOrNull() ?: return false
        val signature = runCatching {
            Base64.getDecoder().decode(manifest.getString(SIGNATURE_FIELD))
        }.getOrNull() ?: return false
        if (signature.size != 64) return false

        val payload = canonicalPayload(manifest).toByteArray(StandardCharsets.UTF_8)
        return verifyPayload(payload, signature, publicKey)
    }

    fun verifyPayload(payload: ByteArray, signature: ByteArray, publicKey: ByteArray): Boolean {
        if (publicKey.size != 32 || signature.size != 64) return false
        val verifier = Ed25519Signer()
        verifier.init(false, Ed25519PublicKeyParameters(publicKey, 0))
        verifier.update(payload, 0, payload.size)
        return verifier.verifySignature(signature)
    }

    fun canonicalPayload(manifest: JSONObject): String = canonicalObject(manifest, omitSignature = true)

    private fun decodePublicKey(value: String): ByteArray {
        val decoded = Base64.getDecoder().decode(value)
        return when (decoded.size) {
            32 -> decoded
            // SubjectPublicKeyInfo prefix for Ed25519, accepting a standard X.509 key too.
            44 -> decoded.copyOfRange(12, 44)
            else -> error("Ed25519 public key must be 32 raw or 44 X.509 bytes")
        }
    }

    private fun canonicalObject(objectValue: JSONObject, omitSignature: Boolean): String {
        return objectValue.keys().asSequence()
            .filterNot { omitSignature && it == SIGNATURE_FIELD }
            .sorted()
            .joinToString(prefix = "{", postfix = "}") { key ->
                JSONObject.quote(key) + ":" + canonicalValue(objectValue.get(key))
            }
    }

    private fun canonicalValue(value: Any?): String = when (value) {
        JSONObject.NULL -> "null"
        is JSONObject -> canonicalObject(value, omitSignature = false)
        is JSONArray -> value.toList().joinToString(prefix = "[", postfix = "]") { canonicalValue(it) }
        is Number, is Boolean -> value.toString()
        else -> JSONObject.quote(value.toString())
    }

    private fun JSONArray.toList(): List<Any?> = buildList(length()) {
        repeat(length()) { add(get(it)) }
    }
}
