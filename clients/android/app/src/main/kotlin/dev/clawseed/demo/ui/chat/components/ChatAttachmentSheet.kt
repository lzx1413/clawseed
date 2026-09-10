package dev.clawseed.demo.ui.chat.components

import android.Manifest
import android.content.ContentUris
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Build
import android.provider.MediaStore
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.selection.toggleable
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.path
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.unit.dp
import androidx.core.content.ContextCompat
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleEventObserver
import androidx.lifecycle.compose.LocalLifecycleOwner
import coil3.compose.AsyncImage
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

@OptIn(ExperimentalMaterial3Api::class)
@Composable
internal fun ChatAttachmentSheet(
    onDismiss: () -> Unit,
    canPickImages: Boolean,
    canPickFiles: Boolean,
    imageSlots: Int,
    onPickImages: (() -> Unit)?,
    onPickFiles: (() -> Unit)?,
    onTakePhoto: (() -> Unit)?,
    onSelectRecent: ((List<Uri>) -> Unit)?,
) {
    val context = LocalContext.current
    val lifecycleOwner = LocalLifecycleOwner.current
    var refresh by remember { mutableIntStateOf(0) }
    var recent by remember { mutableStateOf<List<Uri>>(emptyList()) }
    var selected by remember { mutableStateOf<List<Uri>>(emptyList()) }
    var loading by remember { mutableStateOf(true) }
    var canRead by remember { mutableStateOf(false) }
    var fullAccess by remember { mutableStateOf(false) }
    val permissions = remember {
        when {
            Build.VERSION.SDK_INT >= 34 -> arrayOf(Manifest.permission.READ_MEDIA_IMAGES, Manifest.permission.READ_MEDIA_VISUAL_USER_SELECTED)
            Build.VERSION.SDK_INT >= 33 -> arrayOf(Manifest.permission.READ_MEDIA_IMAGES)
            else -> arrayOf(Manifest.permission.READ_EXTERNAL_STORAGE)
        }
    }
    val permissionLauncher = rememberLauncherForActivityResult(ActivityResultContracts.RequestMultiplePermissions()) { refresh++ }
    DisposableEffect(lifecycleOwner) {
        val observer = LifecycleEventObserver { _, event -> if (event == Lifecycle.Event.ON_RESUME) refresh++ }
        lifecycleOwner.lifecycle.addObserver(observer)
        onDispose { lifecycleOwner.lifecycle.removeObserver(observer) }
    }
    LaunchedEffect(refresh) {
        loading = true
        fullAccess = ContextCompat.checkSelfPermission(context, permissions.first()) == PackageManager.PERMISSION_GRANTED
        canRead = permissions.any { ContextCompat.checkSelfPermission(context, it) == PackageManager.PERMISSION_GRANTED }
        recent = if (canRead) withContext(Dispatchers.IO) {
            try {
                val collection = MediaStore.Images.Media.EXTERNAL_CONTENT_URI
                context.contentResolver.query(
                    collection,
                    arrayOf(MediaStore.Images.Media._ID),
                    null, null,
                    "${MediaStore.Images.Media.DATE_ADDED} DESC, ${MediaStore.Images.Media._ID} DESC",
                )?.use { cursor ->
                    buildList {
                        while (size < 30 && cursor.moveToNext()) add(ContentUris.withAppendedId(collection, cursor.getLong(0)))
                    }
                }.orEmpty()
            } catch (_: SecurityException) {
                emptyList()
            } catch (_: IllegalArgumentException) {
                emptyList()
            }
        } else emptyList()
        selected = selected.filter { it in recent }.take(imageSlots.coerceAtLeast(0))
        loading = false
    }

    fun launch(action: (() -> Unit)?) {
        onDismiss()
        action?.invoke()
    }

    ModalBottomSheet(
        onDismissRequest = onDismiss,
        sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true),
    ) {
        Column(Modifier.fillMaxWidth().padding(bottom = 24.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
            if (recent.isNotEmpty()) {
                LazyRow(contentPadding = PaddingValues(horizontal = 20.dp), horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                    itemsIndexed(recent, key = { _, uri -> uri.toString() }) { index, uri ->
                        val checked = uri in selected
                        Box(
                            Modifier.size(88.dp).clip(RoundedCornerShape(18.dp))
                                .toggleable(
                                    value = checked,
                                    enabled = canPickImages && onSelectRecent != null && (checked || selected.size < imageSlots),
                                    role = Role.Checkbox,
                                    onValueChange = { selected = if (checked) selected - uri else selected + uri },
                                ),
                        ) {
                            AsyncImage(model = uri, contentDescription = "最近图片 ${index + 1}", contentScale = ContentScale.Crop, modifier = Modifier.fillMaxSize())
                            Surface(
                                modifier = Modifier.align(Alignment.TopEnd).padding(7.dp).size(24.dp),
                                shape = CircleShape,
                                color = if (checked) MaterialTheme.colorScheme.primary else Color.Black.copy(alpha = 0.35f),
                                border = BorderStroke(2.dp, Color.White),
                            ) {
                                if (checked) Box(contentAlignment = Alignment.Center) {
                                    Text("${selected.indexOf(uri) + 1}", color = MaterialTheme.colorScheme.onPrimary, style = MaterialTheme.typography.labelSmall)
                                }
                            }
                        }
                    }
                }
            } else {
                Box(Modifier.fillMaxWidth().height(88.dp).padding(horizontal = 20.dp), contentAlignment = Alignment.Center) {
                    if (loading) CircularProgressIndicator(Modifier.size(24.dp))
                    else if (!canRead) TextButton(onClick = { permissionLauncher.launch(permissions) }, enabled = canPickImages) {
                        Text("允许访问照片，显示最近图片")
                    } else Text("暂无可访问的图片，可从相册选择", style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
                }
            }
            if (selected.isNotEmpty()) {
                FilledTonalButton(
                    onClick = { onDismiss(); onSelectRecent?.invoke(selected) },
                    enabled = canPickImages && selected.size <= imageSlots && onSelectRecent != null,
                    modifier = Modifier.align(Alignment.End).padding(horizontal = 20.dp),
                ) { Text("添加 ${selected.size} 张图片") }
            } else if (canRead && !fullAccess) {
                TextButton(onClick = { permissionLauncher.launch(permissions) }, modifier = Modifier.align(Alignment.End).padding(horizontal = 12.dp)) {
                    Text("管理可访问的照片")
                }
            }
            Row(Modifier.fillMaxWidth().padding(horizontal = 20.dp), horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                AttachmentAction("拍照", CameraIcon, canPickImages && imageSlots > 0 && onTakePhoto != null, { launch(onTakePhoto) }, Modifier.weight(1f))
                AttachmentAction("相册", GalleryIcon, canPickImages && imageSlots > 0 && onPickImages != null, { launch(onPickImages) }, Modifier.weight(1f))
                AttachmentAction("文件", AttachmentIcon, canPickFiles && onPickFiles != null, { launch(onPickFiles) }, Modifier.weight(1f))
            }
            if (!canPickFiles) Text("当前连接不支持文件附件或暂不可添加", Modifier.padding(horizontal = 20.dp), style = MaterialTheme.typography.bodySmall)
            if (imageSlots <= 0) Text("每条消息最多添加 4 张图片", Modifier.padding(horizontal = 20.dp), style = MaterialTheme.typography.bodySmall)
        }
    }
}

@Composable
private fun AttachmentAction(label: String, icon: ImageVector, enabled: Boolean, onClick: () -> Unit, modifier: Modifier) {
    Surface(onClick = onClick, enabled = enabled, modifier = modifier.height(92.dp), shape = RoundedCornerShape(20.dp), color = MaterialTheme.colorScheme.surfaceContainerHigh) {
        Column(Modifier.fillMaxSize(), horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(8.dp, Alignment.CenterVertically)) {
            Icon(icon, contentDescription = null, modifier = Modifier.size(28.dp))
            Text(label, style = MaterialTheme.typography.titleMedium)
        }
    }
}

private val CameraIcon = ImageVector.Builder("Camera", 24.dp, 24.dp, 24f, 24f).apply {
    path(stroke = SolidColor(Color.Black), strokeLineWidth = 1.8f, strokeLineCap = StrokeCap.Round, strokeLineJoin = StrokeJoin.Round) {
        moveTo(8f, 5f); lineTo(9.5f, 3f); lineTo(14.5f, 3f); lineTo(16f, 5f); lineTo(19f, 5f)
        curveTo(20.7f, 5f, 22f, 6.3f, 22f, 8f); lineTo(22f, 18f); curveTo(22f, 19.7f, 20.7f, 21f, 19f, 21f)
        lineTo(5f, 21f); curveTo(3.3f, 21f, 2f, 19.7f, 2f, 18f); lineTo(2f, 8f); curveTo(2f, 6.3f, 3.3f, 5f, 5f, 5f); close()
        moveTo(16f, 13f); arcTo(4f, 4f, 0f, true, true, 8f, 13f); arcTo(4f, 4f, 0f, true, true, 16f, 13f); close()
    }
}.build()

private val GalleryIcon = ImageVector.Builder("Gallery", 24.dp, 24.dp, 24f, 24f).apply {
    path(stroke = SolidColor(Color.Black), strokeLineWidth = 1.8f, strokeLineCap = StrokeCap.Round, strokeLineJoin = StrokeJoin.Round) {
        moveTo(6f, 3f); lineTo(18f, 3f); curveTo(20f, 3f, 21f, 4f, 21f, 6f); lineTo(21f, 18f); curveTo(21f, 20f, 20f, 21f, 18f, 21f)
        lineTo(6f, 21f); curveTo(4f, 21f, 3f, 20f, 3f, 18f); lineTo(3f, 6f); curveTo(3f, 4f, 4f, 3f, 6f, 3f); close()
        moveTo(3f, 16f); lineTo(9f, 10f); lineTo(20f, 21f)
        moveTo(18f, 8f); arcTo(2f, 2f, 0f, true, true, 14f, 8f); arcTo(2f, 2f, 0f, true, true, 18f, 8f); close()
    }
}.build()
