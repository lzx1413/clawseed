package dev.clawseed.demo.ui.chat

import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.speech.tts.TextToSpeech
import android.speech.tts.UtteranceProgressListener
import android.util.Log
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import java.util.Locale

/**
 * Thin wrapper around Android's [TextToSpeech] for speaking assistant replies aloud.
 *
 * Mirrors the speech-output behavior of Kai (which uses the KMP `nl.marc_apps.tts` library) but
 * uses the platform TTS directly — ClawSeed's Android client is a pure-Android app, so no extra
 * dependency is needed. Exposes [isSpeaking] and [speakingMessageId] so the UI can reflect
 * playback state (per-message stop button, top-bar stop affordance) without polling.
 *
 * Engine binding strategy: the 2-arg `TextToSpeech(context, listener)` constructor relies on the
 * system's default-engine resolution, which on some OEM ROMs (HyperOS/MIUI) returns no default
 * even when engines are installed — `onInit` then reports ERROR and no audio ever plays. To match
 * Kai (which explicitly binds to an engine package), we first try the default engine; on FAILURE
 * we enumerate every installed package that handles the TTS service intent and bind to each
 * explicitly (3-arg constructor) until one inits successfully. Degrades gracefully to a no-op if
 * no engine can be loaded.
 */
class TtsController(context: Context) {

    private val appContext = context.applicationContext

    private val _isSpeaking = MutableStateFlow(false)
    val isSpeaking: StateFlow<Boolean> = _isSpeaking.asStateFlow()

    private val _speakingMessageId = MutableStateFlow<String?>(null)
    val speakingMessageId: StateFlow<String?> = _speakingMessageId.asStateFlow()

    /** True once an engine reported SUCCESS in the init callback. */
    @Volatile private var engineReady = false

    /** Currently bound engine instance (null until the first successful init, or during a retry). */
    private var tts: TextToSpeech? = null

    /** Engine packages already attempted, to avoid retrying. `null` = the default-engine attempt. */
    private val triedEngines = mutableSetOf<String?>()

    /**
     * Set if `onInit` fires synchronously during the `TextToSpeech` constructor, before [tts] is
     * assigned. The constructor call site replays the deferred status once the field is set.
     */
    private var pendingInitStatus: Int? = null

    init {
        startInit(enginePackage = null)
    }

    /** Returns installed TTS engine packages, common high-quality engines ordered first. */
    private fun queryEngines(): List<String> {
        val pm = appContext.packageManager
        val intent = Intent(TextToSpeech.Engine.INTENT_ACTION_TTS_SERVICE)
        @Suppress("DEPRECATION")
        val resolved = pm.queryIntentServices(intent, 0)
        val packages = resolved.mapNotNull { it.serviceInfo?.packageName }.distinct()

        // Prefer well-known engines first so we bind to a full TTS voice rather than a stub.
        val preferred = listOf(
            "com.google.android.tts",          // Google
            "com.xiaomi.mibrain.speech",       // Xiaomi
            "com.iflytek.speechcloud",         // iFlytek
            "com.baidu.duersdk.actionsdk",      // Baidu
            "com.samsung.SMT",                 // Samsung
            "com.huawei.himovie.tts",          // Huawei
        )
        return (preferred.filter { it in packages } + packages).distinct()
    }

    private fun startInit(enginePackage: String?) {
        pendingInitStatus = null
        val listener = TextToSpeech.OnInitListener { status ->
            val engine = tts
            if (engine != null) {
                handleInit(engine, status, enginePackage)
            } else {
                // Synchronous init during the constructor — defer until the field is assigned.
                pendingInitStatus = status
            }
        }
        val instance = if (enginePackage == null) {
            TextToSpeech(appContext, listener)
        } else {
            TextToSpeech(appContext, listener, enginePackage)
        }
        tts = instance
        // Replay a deferred synchronous init now that the field is set.
        pendingInitStatus?.let { status ->
            pendingInitStatus = null
            handleInit(instance, status, enginePackage)
        }
    }

    private fun handleInit(engine: TextToSpeech, status: Int, enginePackage: String?) {
        if (status == TextToSpeech.SUCCESS) {
            configureEngine(engine, enginePackage)
            return
        }
        // This engine failed — release it and try the next candidate explicitly.
        Log.w(TAG, "init ERROR for engine=$enginePackage; trying alternatives")
        engine.shutdown()
        if (tts === engine) tts = null
        triedEngines.add(enginePackage)

        val next = queryEngines().firstOrNull { it !in triedEngines }
        if (next != null) {
            startInit(next)
        } else {
            engineReady = false
            Log.w(TAG, "no TTS engine could be initialized; tried=$triedEngines")
        }
    }

    private fun configureEngine(engine: TextToSpeech, enginePackage: String?) {
        Log.i(TAG, "init SUCCESS engine=$enginePackage defaultEngine=${engine.defaultEngine}")
        engineReady = true

        // Try the system locale; if its voice data is missing, fall back to English so we still
        // produce *some* audio rather than going silent. We keep engineReady=true regardless so
        // speak() is attempted — the engine itself decides whether it can render the text.
        var langResult = engine.setLanguage(Locale.getDefault())
        Log.i(TAG, "setLanguage(${Locale.getDefault()})=$langResult")
        if (langResult == TextToSpeech.LANG_MISSING_DATA ||
            langResult == TextToSpeech.LANG_NOT_SUPPORTED
        ) {
            langResult = engine.setLanguage(Locale.ENGLISH)
            Log.i(TAG, "fallback setLanguage(ENGLISH)=$langResult")
        }

        engine.setOnUtteranceProgressListener(object : UtteranceProgressListener() {
            override fun onStart(utteranceId: String?) {
                Log.i(TAG, "onStart id=$utteranceId")
                _speakingMessageId.value = utteranceId
                _isSpeaking.value = true
            }

            override fun onDone(utteranceId: String?) {
                Log.i(TAG, "onDone id=$utteranceId")
                _isSpeaking.value = false
                _speakingMessageId.value = null
            }

            @Deprecated("Required override", ReplaceWith(""))
            override fun onError(utteranceId: String?) {
                Log.w(TAG, "onError id=$utteranceId")
                _isSpeaking.value = false
                _speakingMessageId.value = null
            }

            override fun onError(utteranceId: String?, errorCode: Int) {
                Log.w(TAG, "onError id=$utteranceId code=$errorCode")
                _isSpeaking.value = false
                _speakingMessageId.value = null
            }
        })

        // Log available voices for diagnostics (engine-dependent).
        val voices = engine.voices
        if (voices.isNullOrEmpty()) {
            Log.i(TAG, "no voices reported by engine")
        } else {
            Log.i(TAG, "voices: " + voices.joinToString(", ") {
                "${it.name}[${it.locale?.toLanguageTag()}]"
            })
        }
    }

    /**
     * Speak [text], replacing any current utterance. [messageId] is used as the utterance id so
     * the UI can tie playback state back to a specific chat entry.
     */
    fun speak(text: String, messageId: String) {
        val engine = tts
        if (text.isBlank()) return
        if (!engineReady || engine == null) {
            Log.w(TAG, "speak ignored: engine not ready")
            return
        }
        Log.i(TAG, "speak id=$messageId len=${text.length}")
        // QUEUE_FLUSH drops anything in progress so a new reply interrupts the previous one.
        engine.speak(text, TextToSpeech.QUEUE_FLUSH, null, messageId)
    }

    /** Stop any in-progress speech and clear playback state. */
    fun stop() {
        tts?.stop()
        _isSpeaking.value = false
        _speakingMessageId.value = null
    }

    /** Release the engine. Call from [androidx.lifecycle.ViewModel.onCleared]. */
    fun shutdown() {
        tts?.let {
            it.stop()
            it.shutdown()
        }
        engineReady = false
        _isSpeaking.value = false
        _speakingMessageId.value = null
    }

    private companion object {
        private const val TAG = "ClawSeedTts"
    }
}
