package dev.clawseed.demo.ui.theme

import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.ui.unit.dp

object ClawSeedShapes {
    val Compact = RoundedCornerShape(6.dp)
    val Field = RoundedCornerShape(14.dp)
    val Card = RoundedCornerShape(20.dp)
    val Pill = RoundedCornerShape(50)
    val Small = RoundedCornerShape(10.dp)
    val Medium = RoundedCornerShape(16.dp)
    val Large = RoundedCornerShape(24.dp)
    val Sheet = RoundedCornerShape(topStart = 28.dp, topEnd = 28.dp)
}

object ClawSeedSpacing {
    val Tiny = 4.dp
    val Small = 8.dp
    val Medium = 12.dp
    val Page = 16.dp
    val Large = 20.dp
    val Group = 24.dp
    val Huge = 32.dp
}
object ClawSeedSizes {
    val TouchTarget = 48.dp
    val Button = 50.dp
    val Row = 52.dp
    val TwoLineRow = 64.dp
    val IconSlot = 32.dp
}
object ClawSeedMotion {
    const val Press = 100
    const val State = 150
    const val Expand = 250
    const val Sheet = 300
}
