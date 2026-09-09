package dev.madari.tv

import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.gestures.BringIntoViewSpec

/** Preserve the viewport for visible controls; reveal only what is clipped. */
@OptIn(ExperimentalFoundationApi::class)
object TvFocusScroll : BringIntoViewSpec {
    override fun calculateScrollDistance(offset: Float, size: Float, containerSize: Float): Float {
        if(containerSize<=0f || size<=0f) return 0f
        val end=offset+size
        if(offset>=0f && end<=containerSize) return 0f
        // A large parent already covering the viewport cannot be made fully visible.
        if(offset<=0f && end>=containerSize) return 0f
        val inset=minOf(24f,containerSize*.035f,(containerSize-size).coerceAtLeast(0f)/2f)
        return if(offset<0f) offset-inset else end-containerSize+inset
    }
}
