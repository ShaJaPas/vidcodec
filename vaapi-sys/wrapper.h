/* Bindgen input for VA-API (libva). Optional headers are gated by CFG_vaapi_* defines. */
#include <va/va.h>

#ifdef CFG_vaapi_drm
#include <va/va_drm.h>
#endif

#ifdef CFG_vaapi_x11
#include <va/va_x11.h>
#endif

#ifdef CFG_vaapi_wayland
#include <va/va_wayland.h>
#endif
