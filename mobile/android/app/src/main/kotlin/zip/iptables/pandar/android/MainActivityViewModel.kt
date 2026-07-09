package zip.iptables.pandar.android

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asSharedFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import zip.iptables.pandar.android.core.di.AppContainer
import zip.iptables.pandar.android.data.auth.AuthEvent
import zip.iptables.pandar.android.data.auth.AuthState

data class MainActivityUiState(
    val authState: AuthState = AuthState.NEEDS_CONFIG,
)

class MainActivityViewModel(private val container: AppContainer) : ViewModel() {

    private val _state = MutableStateFlow(MainActivityUiState())
    val state: StateFlow<MainActivityUiState> = _state.asStateFlow()

    private val _browserEvents = MutableSharedFlow<android.content.Intent>(extraBufferCapacity = 1)
    val browserEvents: SharedFlow<android.content.Intent> = _browserEvents.asSharedFlow()

    private val _toasts = MutableSharedFlow<String>(extraBufferCapacity = 8)
    val toasts: SharedFlow<String> = _toasts.asSharedFlow()

    private val _openUrl = MutableSharedFlow<String>(extraBufferCapacity = 1)
    val openUrl: SharedFlow<String> = _openUrl.asSharedFlow()

    init {
        viewModelScope.launch {
            container.auth.state.collect { auth ->
                _state.value = _state.value.copy(authState = auth)
            }
        }
        viewModelScope.launch {
            container.auth.authErrors.collect { error -> _toasts.emit(error) }
        }
    }

    fun signIn() {
        viewModelScope.launch {
            when (val event = container.auth.signIn()) {
                is AuthEvent.LaunchBrowser -> _browserEvents.emit(event.intent)
                is AuthEvent.Toast -> _toasts.emit(event.message)
            }
        }
    }

    fun launchBrowser(intent: android.content.Intent) {
        viewModelScope.launch { _browserEvents.emit(intent) }
    }

    fun toast(message: String) {
        viewModelScope.launch { _toasts.emit(message) }
    }

    fun signOut() {
        val endSession = container.auth.endSessionUrl()
        container.auth.signOut()
        // Best-effort: open the provider's end-session endpoint so the browser session is also
        // revoked. Tokens are already discarded regardless of whether this succeeds.
        if (!endSession.isNullOrEmpty()) {
            viewModelScope.launch { _openUrl.emit(endSession) }
        }
    }

    fun handleAuthorizationResponse(intent: android.content.Intent) {
        viewModelScope.launch { container.auth.handleAuthorizationResponse(intent) }
    }
}
