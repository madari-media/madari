# Changelog

## [0.4.0](https://github.com/madari-media/madari/compare/v0.3.0...v0.4.0) (2026-09-11)


### Features

* **ios:** add the iOS client, with libmpv for containers AVPlayer cannot open ([31f4053](https://github.com/madari-media/madari/commit/31f4053cd174d69357c1fac135c6b327fae38ac4))
* **profiles:** default addons, profile deletion and desktop/TV parity ([099cc2f](https://github.com/madari-media/madari/commit/099cc2f140b7478bdc6d140947787f1eeb0e5743))
* **torrents:** let clients list managed torrents and play their files ([7ec5ebc](https://github.com/madari-media/madari/commit/7ec5ebc56ba60763cc6c747b147e3349cbc31c44))


### Bug Fixes

* **ci:** bound the xtool probe and fall back to a source build ([6e2b4e7](https://github.com/madari-media/madari/commit/6e2b4e7af76541693703e20c876f199a50660686))
* **ci:** build the app with xtool instead of xcodebuild ([298a566](https://github.com/madari-media/madari/commit/298a56683eebb3ddff421dfe11846543382a8d56))
* **ci:** build the web bundle instead of asserting it is committed ([c5f4491](https://github.com/madari-media/madari/commit/c5f4491ec92299732ebae8809b3c2e1c1678c164))
* **ci:** download xtool from the tag the release actually uses ([2e7d528](https://github.com/madari-media/madari/commit/2e7d5283fcea052fd25f24420c7a777803178c77))
* **ci:** find the app bundle where xtool actually writes it ([f00f60d](https://github.com/madari-media/madari/commit/f00f60d688cee4359b8c4d448ac17c0016f6025f))
* **ci:** find the built app with a portable maxdepth ([da2634c](https://github.com/madari-media/madari/commit/da2634cb708384ec5e75efd1337da6b42c27615f))
* **ci:** grant the release flow's iOS job the permissions it calls for ([a007a35](https://github.com/madari-media/madari/commit/a007a3524d21fbbed3b166435d63db72f5d42204))
* **ci:** point the iOS cross build at Xcode's SDK on macOS ([ba83ed1](https://github.com/madari-media/madari/commit/ba83ed18726c3f0d6a8c4b34d98afe57f8e38671))
* **ci:** use the released xtool binary on a runner with a new enough Swift ([1468781](https://github.com/madari-media/madari/commit/1468781b00a6a2889cd23238f8076c64723eabae))
* **scripts:** name the host cdylib correctly on macOS ([2d97870](https://github.com/madari-media/madari/commit/2d97870bd2d03948f2fa9f2162a0715da9c209e1))
* **scripts:** scope the iOS sysroot to the iOS target ([cb827ad](https://github.com/madari-media/madari/commit/cb827ad68451a1a3a526ddfc811ebaf975824863))
* **scripts:** verify the libmpv checksum without sha256sum ([cd6a9f5](https://github.com/madari-media/madari/commit/cd6a9f51c7237c598685d90f688c492db1e76852))

## [0.3.0](https://github.com/madari-media/madari/compare/v0.2.0...v0.3.0) (2026-09-10)


### Features

* **release:** ship Linux .deb, .rpm and .AppImage for amd64 and arm64 ([7ba2980](https://github.com/madari-media/madari/commit/7ba29801ff87d80cb0d82ac49325f95a5c607f98))
* **release:** ship Linux .deb, .rpm and .AppImage for amd64 and arm64 ([4c9f7e3](https://github.com/madari-media/madari/commit/4c9f7e3afdc41257d38fddeb421082eb69f3cb7d))

## [0.2.0](https://github.com/madari-media/madari/compare/v0.1.0...v0.2.0) (2026-09-10)


### Features

* add profile avatar functionality and update related components ([835006a](https://github.com/madari-media/madari/commit/835006a6f7472db2f2ee28c05e77163b08dc00d0))
* **tv:** browser settings UI, remote control and live player control ([269c1b8](https://github.com/madari-media/madari/commit/269c1b86c678d30898120a50bf3e059791a3f061))
* **tv:** enhance NavigationRail with animated width and improved focus handling ([aca12c2](https://github.com/madari-media/madari/commit/aca12c2f2368cc28af9b77e95ec96bc7c122f690))
* **tv:** restore profile picker look, add profile tile and wallpaper ([97f3764](https://github.com/madari-media/madari/commit/97f3764e97b63afe11c7cdbd547ba1923b6bdb30))


### Bug Fixes

* stop the .gitignore crash-dump rule from hiding source directories ([12bb388](https://github.com/madari-media/madari/commit/12bb388d9be53dfcf4218f4c0a51f3201b5c3e46))
* **tv:** correct and streamline Continue watching ([66699ee](https://github.com/madari-media/madari/commit/66699ee0c8da15dd416a962fdf9c1b94a756b7b9))
* **tv:** match JNI symbols and R8 keep rule to core package ([7dca27d](https://github.com/madari-media/madari/commit/7dca27d1d4464aceb666a04ba5db904e6a6807b7))
