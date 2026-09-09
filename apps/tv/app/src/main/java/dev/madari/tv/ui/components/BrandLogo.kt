package dev.madari.tv.ui.components

import androidx.compose.foundation.Image
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.unit.dp
import dev.madari.tv.R

/** The shared Madari brand mark, reusing the Linux client asset. */
@Composable
fun BrandLogo(modifier: Modifier = Modifier) {
    Image(painterResource(R.drawable.madari_logo), "Madari", modifier.size(42.dp), contentScale = ContentScale.Fit)
}
