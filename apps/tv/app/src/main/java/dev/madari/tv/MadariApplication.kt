package dev.madari.tv

import android.app.Application

/** Registers the SVG decoder used by addon title logos on the shared Coil image loader. */
class MadariApplication : Application(), coil.ImageLoaderFactory {
    override fun newImageLoader(): coil.ImageLoader = coil.ImageLoader.Builder(this)
        .components { add(coil.decode.SvgDecoder.Factory()) }
        .crossfade(160)
        .build()
}
