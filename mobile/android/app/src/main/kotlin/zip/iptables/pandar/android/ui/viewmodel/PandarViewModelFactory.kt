package zip.iptables.pandar.android.ui.viewmodel

import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.viewmodel.initializer
import androidx.lifecycle.viewmodel.viewModelFactory
import zip.iptables.pandar.android.MainActivityViewModel
import zip.iptables.pandar.android.PandarApplication
import zip.iptables.pandar.android.core.di.AppContainer
import zip.iptables.pandar.android.ui.jobs.JobsViewModel
import zip.iptables.pandar.android.ui.printerdetail.PrinterDetailViewModel
import zip.iptables.pandar.android.ui.printers.PrintersViewModel
import zip.iptables.pandar.android.ui.settings.SettingsViewModel

/**
 * Single factory that constructs the app's ViewModels from the [AppContainer] held by
 * [PandarApplication]. Pass to `viewModel(factory = ...)` from any composable.
 */
object PandarViewModelFactory {

    fun create(container: AppContainer): ViewModelProvider.Factory = viewModelFactory {
        initializer { MainActivityViewModel(container) }
        initializer { PrintersViewModel(container) }
        initializer { JobsViewModel(container) }
        initializer { SettingsViewModel(container) }
    }

    fun createDetail(container: AppContainer, printerId: String): ViewModelProvider.Factory = viewModelFactory {
        initializer { PrinterDetailViewModel(container, printerId) }
    }
}
