using System.IO;
using UnrealBuildTool;

/// <summary>
/// The Zuri runtime: zuri.h plus the Rust static library, both placed here by
/// scripts/build-unreal-libs.sh.
/// </summary>
public class ZuriLibrary : ModuleRules
{
	public ZuriLibrary(ReadOnlyTargetRules Target) : base(Target)
	{
		Type = ModuleType.External;

		PublicSystemIncludePaths.Add(Path.Combine(ModuleDirectory, "include"));

		// The system libraries are the ones rustc reports for each target:
		//   cargo rustc -p zuri-ffi --release --target <triple> --crate-type staticlib -- --print native-static-libs
		string Library;
		if (Target.Platform == UnrealTargetPlatform.Mac)
		{
			// rustc: -lSystem -lc -lm, which every Mac link already has.
			Library = Path.Combine("Mac", "libzuri.a");
		}
		else if (Target.Platform == UnrealTargetPlatform.Win64)
		{
			// Untested. rustc: kernel32 ntdll userenv ws2_32 dbghelp, and the DLL CRT Unreal already uses.
			Library = Path.Combine("Win64", "zuri.lib");
			PublicSystemLibraries.AddRange(new string[] { "kernel32.lib", "ntdll.lib", "userenv.lib", "ws2_32.lib", "dbghelp.lib" });
		}
		else if (Target.Platform == UnrealTargetPlatform.Linux)
		{
			// Untested. rustc: -lgcc_s -lutil -lrt -lpthread -lm -ldl -lc.
			Library = Path.Combine("Linux", "libzuri.a");
			PublicSystemLibraries.AddRange(new string[] { "gcc_s", "util", "rt", "pthread", "m", "dl", "c" });
		}
		else
		{
			throw new BuildException("Zuri has no static library for {0}.", Target.Platform);
		}

		string LibraryPath = Path.Combine(ModuleDirectory, "lib", Library);
		if (!File.Exists(LibraryPath))
		{
			throw new BuildException("Zuri: {0} is missing. Build it with scripts/build-unreal-libs.sh from the Zuri repository.", LibraryPath);
		}
		PublicAdditionalLibraries.Add(LibraryPath);
	}
}
