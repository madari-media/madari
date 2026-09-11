import SwiftUI

/// The bundled brand mark.
///
/// Loaded by URL rather than by name: `Image(_:bundle:)` resolves through
/// `PlatformImage(named:)`, and a loose PNG in a SwiftPM resource bundle is not guaranteed
/// to be found that way. This is a hard requirement for the header, so it is loaded
/// explicitly and cached.
@MainActor
enum BrandAsset {
    static let logo: PlatformImage? = {
        guard let url = AppResources.url(forResource: "MadariLogo", withExtension: "png") else {
            DebugLog.write("brand logo missing from the resource bundle")
            return nil
        }
        guard let image = PlatformImage(contentsOfFile: url.path) else {
            DebugLog.write("brand logo could not be decoded")
            return nil
        }
        return image
    }()
}

/// The Madari mark, drawn to stay visible on this interface.
///
/// The asset is dark red (`#b61c20`, luma ~61) on transparency. On the near-black
/// surface every screen here uses, it is present but effectively invisible — which is
/// why the header appeared to have no logo at all. Rather than recolour it as a
/// template and lose its detail, it is shown on a light chip, which is how the same
/// mark is presented as an app icon.
struct BrandLogo: View {
    var size: CGFloat = 28

    var body: some View {
        Group {
            if let logo = BrandAsset.logo {
                Image(platformImage: logo)
                    .resizable()
                    .scaledToFit()
                    .padding(size * 0.2)
            } else {
                // Never leave a gap in the lockup if the asset cannot be read.
                Text("M")
                    .font(MadariFont.bold(size * 0.55))
                    .foregroundStyle(MadariColors.accent)
            }
        }
        .frame(width: size, height: size)
        .background(
            Color.white.opacity(0.94),
            in: RoundedRectangle(cornerRadius: size * 0.26, style: .continuous)
        )
        .accessibilityHidden(true)
    }
}
