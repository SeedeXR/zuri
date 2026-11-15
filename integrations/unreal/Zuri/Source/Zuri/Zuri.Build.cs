using UnrealBuildTool;

public class Zuri : ModuleRules
{
	public Zuri(ReadOnlyTargetRules Target) : base(Target)
	{
		PCHUsage = PCHUsageMode.UseExplicitOrSharedPCHs;
		// Every file compiles on its own, so each one has to include what it uses.
		bUseUnity = false;

		PublicDependencyModuleNames.AddRange(new string[] { "Core", "CoreUObject", "Engine", "Slate", "SlateCore", "UMG" });
		PrivateDependencyModuleNames.AddRange(new string[] { "ImageCore", "InputCore", "ZuriLibrary" });
	}
}
