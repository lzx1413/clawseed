package dev.clawseed.demo.ui.chat

import android.content.Context
import android.content.Intent
import android.provider.Settings
import android.speech.tts.TextToSpeech
import android.speech.tts.UtteranceProgressListener
import android.util.Log
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import java.util.Locale

data class TtsEngineInfo(
    val packageName: String,
    val displayName: String,
    val isSystemDefault: Boolean,
)

enum class TtsTestStatus { IDLE, TESTING, SUCCESS, FAILURE }

data class TtsTestState(
    val packageName: String? = null,
    val status: TtsTestStatus = TtsTestStatus.IDLE,
    val errorMessage: String? = null,
)

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

    private val _activeEnginePackage = MutableStateFlow<String?>(null)
    val activeEnginePackage: StateFlow<String?> = _activeEnginePackage.asStateFlow()

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
        return orderedEnginePackages(appContext)
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
        _activeEnginePackage.value = enginePackage ?: engine.defaultEngine

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
        _activeEnginePackage.value = null
    }

    companion object {
        /** Lists installed TTS engines in the same preference order used by runtime playback. */
        fun installedEngines(context: Context): List<TtsEngineInfo> {
            val packages = orderedEnginePackages(context.applicationContext)
            val defaultPackage = Settings.Secure.getString(
                context.contentResolver,
                Settings.Secure.TTS_DEFAULT_SYNTH,
            )
            val pm = context.packageManager
            return packages.map { packageName ->
                val displayName = runCatching {
                    pm.getApplicationLabel(pm.getApplicationInfo(packageName, 0)).toString()
                }.getOrDefault(packageName)
                TtsEngineInfo(
                    packageName = packageName,
                    displayName = displayName,
                    isSystemDefault = packageName == defaultPackage,
                )
            }
        }

        private fun orderedEnginePackages(context: Context): List<String> {
            val pm = context.packageManager
            val intent = Intent(TextToSpeech.Engine.INTENT_ACTION_TTS_SERVICE)
            @Suppress("DEPRECATION")
            val resolved = pm.queryIntentServices(intent, 0)
            val packages = resolved.mapNotNull { it.serviceInfo?.packageName }.distinct()
            return (PREFERRED_ENGINES.filter { it in packages } + packages).distinct()
        }

        private val PREFERRED_ENGINES = listOf(
            "com.google.android.tts",
            "com.xiaomi.mibrain.speech",
            "com.iflytek.speechcloud",
            "com.baidu.duersdk.actionsdk",
            "com.samsung.SMT",
            "com.huawei.himovie.tts",
        )
        private const val TAG = "ClawSeedTts"
    }
}

/** Plays a short sample through one explicitly selected engine without changing chat playback. */
class TtsEngineTester(context: Context) {
    private val appContext = context.applicationContext
    private val _state = MutableStateFlow(TtsTestState())
    val state: StateFlow<TtsTestState> = _state.asStateFlow()

    private var tts: TextToSpeech? = null
    private var pendingInitStatus: Int? = null

    fun test(engine: TtsEngineInfo, text: String) {
        stop()
        _state.value = TtsTestState(engine.packageName, TtsTestStatus.TESTING)
        val utteranceId = "tts-test-${System.nanoTime()}"
        val listener = TextToSpeech.OnInitListener { status ->
            val instance = tts
            if (instance == null) {
                pendingInitStatus = status
            } else {
                handleInit(instance, status, engine.packageName, utteranceId, text)
            }
        }
        val instance = runCatching { TextToSpeech(appContext, listener, engine.packageName) }
            .getOrElse {
                _state.value = TtsTestState(engine.packageName, TtsTestStatus.FAILURE, it.message)
                return
            }
        tts = instance
        pendingInitStatus?.let { status ->
            pendingInitStatus = null
            handleInit(instance, status, engine.packageName, utteranceId, text)
        }
    }

    private fun handleInit(
        engine: TextToSpeech,
        status: Int,
        packageName: String,
        utteranceId: String,
        text: String,
    ) {
        if (status != TextToSpeech.SUCCESS) {
            engine.shutdown()
            if (tts === engine) tts = null
            _state.value = TtsTestState(packageName, TtsTestStatus.FAILURE, "初始化失败")
            return
        }
        val language = engine.setLanguage(Locale.getDefault())
        if (language == TextToSpeech.LANG_MISSING_DATA || language == TextToSpeech.LANG_NOT_SUPPORTED) {
            engine.setLanguage(Locale.ENGLISH)
        }
        engine.setOnUtteranceProgressListener(object : UtteranceProgressListener() {
            override fun onStart(utteranceId: String?) = Unit

            override fun onDone(utteranceId: String?) {
                _state.value = TtsTestState(packageName, TtsTestStatus.SUCCESS)
            }

            @Deprecated("Required override")
            override fun onError(utteranceId: String?) {
                _state.value = TtsTestState(packageName, TtsTestStatus.FAILURE, "播放失败")
            }

            override fun onError(utteranceId: String?, errorCode: Int) {
                _state.value = TtsTestState(packageName, TtsTestStatus.FAILURE, "播放失败 ($errorCode)")
            }
        })
        val result = engine.speak(text, TextToSpeech.QUEUE_FLUSH, null, utteranceId)
        if (result == TextToSpeech.ERROR) {
            _state.value = TtsTestState(packageName, TtsTestStatus.FAILURE, "引擎拒绝播放")
        }
    }

    fun stop() {
        tts?.stop()
        tts?.shutdown()
        tts = null
        pendingInitStatus = null
        if (_state.value.status == TtsTestStatus.TESTING) {
            _state.value = TtsTestState()
        }
    }

    fun shutdown() = stop()
}
