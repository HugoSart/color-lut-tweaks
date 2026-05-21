# Xiaomi G Pro 27i Guide

This project also includes a default Xiaomi G Pro 27i HDR EOTF curve correction (because this is what motivated me to
create this tool) and Native to sRGB lut. You can use it by simply starting the application and selecting the desired
preset. If the monitor device id is not 0, click on the "Edit" button and manually edit the device number.


![tray-screenshot.png](images/tray-screenshot.png)

### Presets:
- Xiaomi G Pro 27i CHIMOLOG Calibration:
    - Apply Native -> SRGB color conversion for SDR usage;
    - Apply EOTF correction for HDR usage;
- Xiaomi G Pro 27i CHIMOLOG Calibration (More Contrast):
    - Same as above;
    - Boost contrast multiplier to 1.2 (nice for desktop usage but affects peak brightness);

### Recommended settings:

Settings to toggle on the monitor settings, windows and GPU drivers for the best experience.

The first recommendation is: use desktop in SDR mode always, and switch to HDR only before launching movies / games 
(Win + Alt + B). Also keep Local Dimming disabled for desktop usage, and enable it before launching movies / games.

- **Monitor Settings:**
    - **FreeSync:** On
    - **SDR:**
        - **Contrast:** 65
        - **Color Temperature (choose one):**
            - Standard
            - 53, 50, 47
            - 50, 49, 49
        - **Hue:** 50
        - **Saturation:** 50
        - **Gamma:** 2.2
        - **Response Time:** Fastest
        - **Sharpness:** 50
        - **Saturation:** 50
        - **DCR:** Off
        - **Color Space:** Native (IMPORTANT)
        - **Local Dimming:** Off for Desktop, On for Movies / Games
    - **HDR:**
        - **Mode:** AUTO
        - **Local Dimming:** HIGH for Desktop and Movies / Games
- **Windows:**
    - **Auto Color Management:** DISABLED!!
    - **Color Profiles:**
        - **SDR:** None
        - **HDR:** [Xiaomi G Pro 27i HDR DisplayCal.icm](https://github.com/HugoSart/color-lut-tweaks/tree/main/profiles)
- **NVIDIA:**
    - **System > Color Accuracy Mode:** DISABLED!!
    - **System > Color:**
        - **Output Color Settings:** NVIDIA
        - **Desktop Color Depth:** Highest (32-bit)
        - **Output Color Depth:** 10 bpc
        - **Output Color Format:** RGB
        - **Output Dynamic Range:** Full
    - **System > Color Channel:**
        - **Brightness:** 100
        - **Contrast:** 100
        - **Gamma:** 1
        - **Digital Vibrance:** 50 ~ 55
        - **Hue** 0
- **AMD / Intel Graphics:**
    - TODO