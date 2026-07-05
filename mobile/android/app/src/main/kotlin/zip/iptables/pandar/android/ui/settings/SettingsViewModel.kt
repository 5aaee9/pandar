package zip.iptables.pandar.android.ui.settings

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.launchIn
import kotlinx.coroutines.flow.onEach
import kotlinx.coroutines.launch
import zip.iptables.pandar.android.core.di.AppContainer
import zip.iptables.pandar.android.data.auth.AuthState
import zip.iptables.pandar.android.data.settings.SettingsSnapshot

data class SettingsUiState(
    val snapshot: SettingsSnapshot = SettingsSnapshot(),
    val authState: AuthState = AuthState.NEEDS_CONFIG,
    val saved: Boolean = false,
    val identitySubject: String? = null,
    val identityIssuer: String? = null,
    val endSessionUrl: String? = null,
)

class SettingsViewModel(private val container: AppContainer) : ViewModel() {

    private val _state = MutableStateFlow(SettingsUiState())
    val state: StateFlow<SettingsUiState> = _state.asStateFlow()

    private val draft = MutableStateFlow(SettingsSnapshot())

    init {
        container.settings.settings.onEach { snap ->
            draft.value = snap
            _state.value = _state.value.copy(
                snapshot = snap,
                authState = container.auth.state.value,
                identitySubject = container.auth.identity.value?.subject,
                identityIssuer = container.auth.identity.value?.issuer,
                endSessionUrl = container.auth.endSessionUrl(),
            )
        }.launchIn(viewModelScope)

        container.auth.state.onEach { auth ->
            _state.value = _state.value.copy(authState = auth)
        }.launchIn(viewModelScope)

        container.auth.identity.onEach { identity ->
            _state.value = _state.value.copy(
                identitySubject = identity?.subject,
                identityIssuer = identity?.issuer,
            )
        }.launchIn(viewModelScope)
    }

    fun edit(transform: (SettingsSnapshot) -> SettingsSnapshot) {
        draft.value = transform(draft.value)
        _state.value = _state.value.copy(snapshot = draft.value, saved = false)
    }

    fun save() {
        viewModelScope.launch {
            container.settings.update { draft.value }
            _state.value = _state.value.copy(saved = true)
        }
    }

    fun signIn() {
        // Sign-in is launched by the host activity (it must collect the browser-launch event).
    }

    fun signOut() {
        container.auth.signOut()
    }
}
