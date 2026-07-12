package zip.iptables.pandar.android.ui.navigation

import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Build
import androidx.compose.material.icons.filled.List
import androidx.compose.material.icons.filled.Settings
import androidx.compose.material3.Icon
import androidx.compose.material3.NavigationBar
import androidx.compose.material3.NavigationBarItem
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.foundation.layout.padding
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.LocalContext
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import androidx.navigation.NavHostController
import androidx.navigation.compose.NavHost
import androidx.navigation.compose.composable
import androidx.navigation.compose.currentBackStackEntryAsState
import androidx.navigation.compose.rememberNavController
import zip.iptables.pandar.android.MainActivityViewModel
import zip.iptables.pandar.android.PandarApplication
import zip.iptables.pandar.android.data.auth.AuthState
import zip.iptables.pandar.android.ui.jobs.JobsScreen
import zip.iptables.pandar.android.ui.jobs.JobsViewModel
import zip.iptables.pandar.android.ui.login.LoginScreen
import zip.iptables.pandar.android.ui.printerdetail.PrinterDetailScreen
import zip.iptables.pandar.android.ui.printerdetail.PrinterDetailViewModel
import zip.iptables.pandar.android.ui.printers.PrintersScreen
import zip.iptables.pandar.android.ui.printers.PrintersViewModel
import zip.iptables.pandar.android.ui.settings.SettingsScreen
import zip.iptables.pandar.android.ui.settings.SettingsViewModel
import zip.iptables.pandar.android.ui.viewmodel.PandarViewModelFactory

object Routes {
    const val PRINTERS = "printers"
    const val PRINTER_DETAIL = "printers/{printerId}"
    const val JOBS = "jobs"
    const val SETTINGS = "settings"

    fun printerDetail(id: String) = "printers/$id"
}

private data class BottomItem(val route: String, val label: String, val icon: ImageVector)

private val bottomItems = listOf(
    BottomItem(Routes.PRINTERS, "Printers", Icons.Default.List),
    BottomItem(Routes.JOBS, "Jobs", Icons.Default.Build),
    BottomItem(Routes.SETTINGS, "Settings", Icons.Default.Settings),
)

@Composable
fun PandarNavGraph(mainVm: MainActivityViewModel) {
    val navController = rememberNavController()
    val container = (LocalContext.current.applicationContext as PandarApplication).container
    val mainState by mainVm.state.collectAsStateWithLifecycle()

    when (mainState.authState) {
        AuthState.NEEDS_CONFIG -> {
            val vm: SettingsViewModel = viewModel(factory = PandarViewModelFactory.create(container))
            val uiState by vm.state.collectAsStateWithLifecycle()
            SettingsScreen(
                state = uiState,
                onEdit = { transform -> vm.edit(transform) },
                onSave = { vm.save() },
                onSignIn = { },
                onSignOut = { vm.signOut() },
            )
            return
        }
        AuthState.SIGNED_OUT, AuthState.SIGNING_IN -> {
            val latestToast by mainVm.toasts.collectAsState(initial = null)
            LoginScreen(
                state = mainState.authState,
                onSignIn = { mainVm.signIn() },
                onLaunchBrowser = { mainVm.launchBrowser(it) },
                onToast = { mainVm.toast(it) },
                errorMessage = latestToast,
            )
            return
        }
        else -> Unit
    }

    val backStack by navController.currentBackStackEntryAsState()
    val currentRoute = backStack?.destination?.route
    val showBottom = currentRoute in bottomItems.map { it.route }

    Scaffold(
        bottomBar = {
            if (showBottom) {
                NavigationBar {
                    bottomItems.forEach { item ->
                        NavigationBarItem(
                            selected = currentRoute == item.route,
                            onClick = {
                                navController.navigate(item.route) {
                                    popUpTo(Routes.PRINTERS) { saveState = true }
                                    launchSingleTop = true
                                }
                            },
                            icon = { Icon(item.icon, contentDescription = item.label) },
                            label = { Text(item.label) },
                        )
                    }
                }
            }
        },
    ) { padding ->
        NavHost(
            navController = navController,
            startDestination = Routes.PRINTERS,
            modifier = Modifier.padding(padding),
        ) {
            composable(Routes.PRINTERS) {
                val vm: PrintersViewModel = viewModel(factory = PandarViewModelFactory.create(container))
                val uiState by vm.state.collectAsStateWithLifecycle()
                PrintersScreen(
                    state = uiState,
                    onOpenPrinter = { navController.navigate(Routes.printerDetail(it)) },
                    onRefresh = { vm.refresh() },
                )
            }
            composable(Routes.PRINTER_DETAIL) { entry ->
                val printerId = entry.arguments?.getString("printerId").orEmpty()
                val vm: PrinterDetailViewModel = viewModel(
                    factory = PandarViewModelFactory.createDetail(container, printerId),
                )
                val uiState by vm.state.collectAsStateWithLifecycle()
                PrinterDetailScreen(
                    state = uiState,
                    onPause = { vm.pause() },
                    onResume = { vm.resume() },
                    onStop = { vm.stop() },
                    onToggleLight = { vm.toggleLight() },
                    onHome = { vm.home() },
                    onMoveAxis = { axis, deltaMm -> vm.moveAxis(axis, deltaMm) },
                    onSetChamberLight = { vm.setChamberLight(it) },
                    onSetHotend = { vm.setHotend(it, false, null) },
                    onSetBed = { vm.setBed(it, false) },
                    onSetChamber = { vm.setChamber(it, false) },
                    onAmsLoad = { vm.amsLoad(it) },
                    onAmsUnload = { vm.amsUnload(it) },
                    onAmsReread = { vm.amsReread(it.amsId, it.slotId) },
                )
            }
            composable(Routes.JOBS) {
                val vm: JobsViewModel = viewModel(factory = PandarViewModelFactory.create(container))
                val uiState by vm.state.collectAsStateWithLifecycle()
                JobsScreen(state = uiState, onRetry = { vm.retry(it) }, onReprint = { vm.reprint(it) })
            }
            composable(Routes.SETTINGS) {
                val vm: SettingsViewModel = viewModel(factory = PandarViewModelFactory.create(container))
                val uiState by vm.state.collectAsStateWithLifecycle()
                SettingsScreen(
                    state = uiState,
                    onEdit = { transform -> vm.edit(transform) },
                    onSave = { vm.save() },
                    onSignIn = { mainVm.signIn() },
                    onSignOut = { mainVm.signOut() },
                )
            }
        }
    }
}
