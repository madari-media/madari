package dev.madari.tv.ui.components

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusDirection
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.input.key.type
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.text.input.VisualTransformation
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.tv.material3.Button
import androidx.tv.material3.ButtonDefaults
import androidx.tv.material3.MaterialTheme
import androidx.tv.material3.Text
import dev.madari.tv.ui.theme.TvColors

/** Primary focusable action button used throughout the TV interface. */
@Composable
fun Action(
    label: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
    icon: String? = null,
    primary: Boolean = false
) {
    Button(
        onClick = onClick, modifier = modifier, enabled = enabled,
        shape = ButtonDefaults.shape(shape = RoundedCornerShape(6.dp)),
        scale = ButtonDefaults.scale(focusedScale = 1.04f),
        colors = ButtonDefaults.colors(
            containerColor = if (primary) Color.White else TvColors.Panel,
            contentColor = if (primary) TvColors.Background else Color.White,
            focusedContainerColor = Color.White, focusedContentColor = TvColors.Background
        )
    ) {
        if (icon != null) { Glyph(icon); Spacer(Modifier.width(8.dp)) }
        Text(label, style = MaterialTheme.typography.labelLarge)
    }
}

@Composable fun Heading(text: String, modifier: Modifier = Modifier) =
    Text(text, modifier, style = MaterialTheme.typography.headlineMedium)

@Composable fun Hint(text: String, modifier: Modifier = Modifier) =
    Text(text, modifier, color = TvColors.Muted, style = MaterialTheme.typography.bodyLarge)

/** Labelled text field with D-pad focus styling and optional secret input. */
@Composable
fun Input(
    label: String,
    value: String,
    onChange: (String) -> Unit,
    modifier: Modifier = Modifier,
    secret: Boolean = false
) {
    var focused by remember { mutableStateOf(false) }
    val focus=LocalFocusManager.current
    Column(modifier, verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Text(label, style = MaterialTheme.typography.bodyLarge)
        BasicTextField(
            value, onChange, singleLine = true,
            textStyle = TextStyle(color = Color.White, fontSize = 18.sp),
            cursorBrush = SolidColor(TvColors.Accent),
            keyboardOptions = KeyboardOptions(keyboardType = if (secret) KeyboardType.NumberPassword else KeyboardType.Text),
            keyboardActions = KeyboardActions(onDone = { focus.moveFocus(FocusDirection.Down) }),
            visualTransformation = if (secret) PasswordVisualTransformation() else VisualTransformation.None,
            modifier = Modifier.fillMaxWidth().onFocusChanged { focused = it.isFocused }
                // A single-line field consumes D-pad KeyDown for cursor movement but not
                // KeyUp, so move focus on KeyUp to avoid trapping focus on TV.
                .onPreviewKeyEvent { event ->
                    if (event.type == KeyEventType.KeyUp && (event.key == Key.DirectionUp || event.key == Key.DirectionDown)) {
                        focus.moveFocus(if (event.key == Key.DirectionDown) FocusDirection.Down else FocusDirection.Up)
                        true
                    } else false
                }
                .semantics { contentDescription = label }
                .background(TvColors.Panel, RoundedCornerShape(6.dp))
                .border(if (focused) 2.dp else 1.dp, if (focused) Color.White else Color.White.copy(.15f), RoundedCornerShape(6.dp))
                .padding(14.dp)
        )
    }
}
