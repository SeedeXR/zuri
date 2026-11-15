using UnrealBuildTool;

public class ZuriEditor : ModuleRules
{
	public ZuriEditor(ReadOnlyTargetRules Target) : base(Target)
	{
		PCHUsage = PCHUsageMode.UseExplicitOrSharedPCHs;
		// Every file compiles on its own, so each one has to include what it uses.
		bUseUnity = false;

		PrivateDependencyModuleNames.AddRange(new string[] { "AssetDefinition", "Core", "CoreUObject", "Engine", "Projects", "UnrealEd", "Zuri" });
	}
}
