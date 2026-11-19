use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::System::Variant::VARIANT;
use windows::Win32::UI::Accessibility::{
    IRangeValueProvider, IRangeValueProvider_Impl, IRawElementProviderSimple,
    IRawElementProviderSimple_Impl, ProviderOptions, ProviderOptions_ServerSideProvider,
    UIA_ControlTypePropertyId, UIA_IsKeyboardFocusablePropertyId, UIA_NamePropertyId,
    UIA_PATTERN_ID, UIA_PROPERTY_ID, UIA_RangeValuePatternId, UIA_SliderControlTypeId,
    UiaHostProviderFromHwnd, UiaReturnRawElementProvider,
};
use windows::Win32::UI::WindowsAndMessaging::PostMessageW;
use windows::core::{BOOL, BSTR, Error, IUnknown, IUnknownImpl, Interface, Result, implement};

use super::slider::{WM_SLIDE_TO, snapshot};

#[implement(IRawElementProviderSimple, IRangeValueProvider)]
struct Slider {
    window: isize,
}

impl Slider {
    fn hwnd(&self) -> HWND {
        HWND(self.window as _)
    }

    fn range(&self) -> Result<(i32, i32, i32)> {
        let track = snapshot(self.hwnd()).ok_or_else(Error::empty)?;
        Ok((track.low, track.high, track.value))
    }
}

impl IRawElementProviderSimple_Impl for Slider_Impl {
    fn ProviderOptions(&self) -> Result<ProviderOptions> {
        Ok(ProviderOptions_ServerSideProvider)
    }

    fn GetPatternProvider(&self, pattern: UIA_PATTERN_ID) -> Result<IUnknown> {
        if pattern == UIA_RangeValuePatternId {
            return self.to_interface::<IRangeValueProvider>().cast();
        }
        Err(Error::empty())
    }

    fn GetPropertyValue(&self, property: UIA_PROPERTY_ID) -> Result<VARIANT> {
        let name = snapshot(self.hwnd())
            .map(|track| track.name)
            .unwrap_or_default();
        match property {
            id if id == UIA_ControlTypePropertyId => Ok(VARIANT::from(UIA_SliderControlTypeId.0)),
            id if id == UIA_NamePropertyId => Ok(VARIANT::from(BSTR::from(name))),
            id if id == UIA_IsKeyboardFocusablePropertyId => Ok(VARIANT::from(true)),
            _ => Ok(VARIANT::default()),
        }
    }

    fn HostRawElementProvider(&self) -> Result<IRawElementProviderSimple> {
        unsafe { UiaHostProviderFromHwnd(self.hwnd()) }
    }
}

impl IRangeValueProvider_Impl for Slider_Impl {
    fn SetValue(&self, value: f64) -> Result<()> {
        unsafe {
            PostMessageW(
                Some(self.hwnd()),
                WM_SLIDE_TO,
                WPARAM(value.round() as usize),
                LPARAM(0),
            )
        }
    }

    fn Value(&self) -> Result<f64> {
        self.range().map(|(_, _, value)| f64::from(value))
    }

    fn IsReadOnly(&self) -> Result<BOOL> {
        Ok(BOOL(0))
    }

    fn Maximum(&self) -> Result<f64> {
        self.range().map(|(_, high, _)| f64::from(high))
    }

    fn Minimum(&self) -> Result<f64> {
        self.range().map(|(low, _, _)| f64::from(low))
    }

    fn LargeChange(&self) -> Result<f64> {
        self.range()
            .map(|(low, high, _)| f64::from(((high - low) / 10).max(1)))
    }

    fn SmallChange(&self) -> Result<f64> {
        Ok(1.0)
    }
}

pub fn provider_for(
    hwnd: HWND,
    wparam: WPARAM,
    lparam: LPARAM,
) -> windows::Win32::Foundation::LRESULT {
    let provider: IRawElementProviderSimple = Slider {
        window: hwnd.0 as isize,
    }
    .into();
    unsafe { UiaReturnRawElementProvider(hwnd, wparam, lparam, &provider) }
}
