package dev.clawseed.demo.i18n

import androidx.compose.runtime.Composable
import androidx.compose.ui.res.stringResource
import androidx.annotation.StringRes
import dev.clawseed.demo.R
import dev.clawseed.demo.datatransfer.DataCategory
import dev.clawseed.demo.datatransfer.ImportStrategy
import dev.clawseed.demo.scheduled.TaskRepeat

/** Composable extensions for resolving enum @StringRes labels in UI context. */

@Composable
fun DataCategory.label(): String = stringResource(labelRes)

@Composable
fun DataCategory.desc(): String = stringResource(descriptionRes)

@Composable
fun ImportStrategy.label(): String = stringResource(labelRes)

@Composable
fun TaskRepeat.label(): String = stringResource(labelRes)

/** Non-Composable extensions for resolving enum labels with a Context. */

fun DataCategory.label(context: android.content.Context): String = context.getString(labelRes)

fun DataCategory.desc(context: android.content.Context): String = context.getString(descriptionRes)

fun ImportStrategy.label(context: android.content.Context): String = context.getString(labelRes)

fun TaskRepeat.label(context: android.content.Context): String = context.getString(labelRes)
