package zip.iptables.pandar.android.ui.login

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asSharedFlow
import kotlinx.coroutines.launch
import zip.iptables.pandar.android.data.auth.AuthEvent
import zip.iptables.pandar.android.data.auth.AuthRepository
import zip.iptables.pandar.android.data.auth.AuthState

class LoginViewModel(private val auth: AuthRepository) : ViewModel() {

    val state: StateFlow<AuthState> = auth.state

    private val _events = MutableSharedFlow<AuthEvent>(extraBufferCapacity = 8)
    val events: SharedFlow<AuthEvent> = _events.asSharedFlow()

    fun signIn() {
        viewModelScope.launch {
            _events.emit(auth.signIn())
        }
    }
}
