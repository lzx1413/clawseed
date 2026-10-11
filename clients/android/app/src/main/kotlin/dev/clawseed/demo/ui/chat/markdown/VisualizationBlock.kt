package dev.clawseed.demo.ui.chat.markdown

import android.annotation.SuppressLint
import android.webkit.WebView
import android.webkit.WebViewClient
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put

/** Render a validated visualization in a local, network-disabled Chart.js WebView. */
@SuppressLint("SetJavaScriptEnabled")
@Composable
internal fun VisualizationBlock(
    visualization: Visualization,
    modifier: Modifier = Modifier,
) {
    val config = visualization.config
    if (!visualization.closed || config == null) {
        CodeFenceBlock(
            language = "visualize",
            code = visualization.raw,
            modifier = modifier.padding(vertical = 4.dp),
        )
        return
    }

    val html = remember(config) { buildVisualizationHtml(config) }
    Surface(
        modifier = modifier.fillMaxWidth().padding(vertical = 4.dp),
        shape = RoundedCornerShape(8.dp),
        color = MaterialTheme.colorScheme.surfaceVariant,
    ) {
        Column {
            config.title?.takeIf { it.isNotBlank() }?.let {
                Text(
                    text = it,
                    style = MaterialTheme.typography.titleSmall,
                    modifier = Modifier.padding(start = 12.dp, top = 10.dp, end = 12.dp),
                )
            }
            AndroidView(
                factory = { viewContext ->
                    WebView(viewContext).apply {
                        settings.javaScriptEnabled = true
                        settings.domStorageEnabled = false
                        settings.allowFileAccess = false
                        settings.allowContentAccess = false
                        settings.blockNetworkLoads = true
                        settings.allowFileAccessFromFileURLs = false
                        settings.allowUniversalAccessFromFileURLs = false
                        webViewClient = WebViewClient()
                        setBackgroundColor(android.graphics.Color.TRANSPARENT)
                    }
                },
                update = { webView ->
                    if (webView.tag != html) {
                        webView.tag = html
                        webView.loadDataWithBaseURL(
                            "file:///android_asset/visualize/",
                            html,
                            "text/html",
                            "UTF-8",
                            null,
                        )
                    }
                },
                modifier = Modifier
                    .fillMaxWidth()
                    .height(260.dp)
                    .clip(RoundedCornerShape(8.dp)),
            )
        }
    }
}

private fun buildVisualizationHtml(config: VisualizationConfig): String {
    val data = buildJsonObject {
        put("labels", kotlinx.serialization.json.buildJsonArray {
            config.labels.forEach { add(JsonPrimitive(it)) }
        })
        put("datasets", kotlinx.serialization.json.buildJsonArray {
            config.datasets.forEach { dataset ->
                add(buildJsonObject {
                    put("label", dataset.label)
                    put("data", kotlinx.serialization.json.buildJsonArray {
                        dataset.data.forEach { add(JsonPrimitive(it)) }
                    })
                    dataset.color?.let { color ->
                        put("borderColor", color)
                        put("backgroundColor", color)
                    }
                    put("borderWidth", 2)
                    put("tension", 0.25)
                })
            }
        })
    }
    val dataJson = Json.encodeToString(JsonObject.serializer(), data)
        .replace("<", "\\u003c")
        .replace(">", "\\u003e")
        .replace("&", "\\u0026")
    val type = config.type
    val isCircular = type == "pie" || type == "doughnut"
    val scales = if (isCircular) "{}" else "{ x: { ticks: { color: '#777' } }, y: { beginAtZero: true, ticks: { color: '#777' } } }"
    return """
        <!doctype html>
        <html><head><meta name="viewport" content="width=device-width, initial-scale=1">
        <script src="chart.umd.js"></script>
        <style>html,body{margin:0;padding:0;background:transparent}canvas{width:100%!important;height:240px!important}</style>
        </head><body><canvas id="chart"></canvas><script>
        const payload = $dataJson;
        const circular = $isCircular;
        new Chart(document.getElementById('chart'), {
          type: '$type',
          data: payload,
          options: {
            responsive: true,
            maintainAspectRatio: false,
            animation: false,
            plugins: { legend: { display: true, position: 'bottom' } },
            scales: $scales
          }
        });
        </script></body></html>
    """.trimIndent()
}
