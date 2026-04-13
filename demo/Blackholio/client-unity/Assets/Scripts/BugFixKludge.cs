using System.Collections;
using System.Collections.Generic;
using UnityEngine;

﻿#if !NET5_0_OR_GREATER
namespace System.Runtime.CompilerServices
{    
  internal static class IsExternalInit { } // https://stackoverflow.com/a/64749403/1484415
}
#endif