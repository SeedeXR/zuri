using UnrealBuildTool;

public class ZuriEditor : ModuleRules
{
	public ZuriEditor(ReadOnlyTargetRules Target) : base(Target)
	{
		PCHUsage = PCHUsageMode.UseExplicitOrSharedPCHs;
		// Every file compiles on its own, so each one has to include what it uses.
		bUseUnity = false;

		PrivateDependencyModuleNames.AddRange(new string[] { "AssetDefinition", "AudioEditor", "Core", "CoreUObject", "Engine", "Projects", "UnrealEd", "Zuri" });
		// For the tests that draw.
		PrivateDependencyModuleNames.AddRange(new string[] { "InputCore", "RenderCore", "RHI", "Slate", "SlateCore", "UMG" });
	}
}
