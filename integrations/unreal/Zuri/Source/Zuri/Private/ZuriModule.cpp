#include "Modules/ModuleInterface.h"
#include "Modules/ModuleManager.h"
#include "ZuriRuntime.h"

DEFINE_LOG_CATEGORY(LogZuri);

class FZuriModule : public IModuleInterface
{
public:
	virtual void StartupModule() override
	{
		// A library left over from an older checkout would otherwise fail in stranger ways later.
		if (zuri_abi_version() != ZURI_ABI_VERSION)
		{
			UE_LOG(LogZuri, Error, TEXT("The linked Zuri library has ABI version %u but the plugin expects %d. Rebuild it with scripts/build-unreal-libs.sh."), zuri_abi_version(), ZURI_ABI_VERSION);
		}
	}
};

IMPLEMENT_MODULE(FZuriModule, Zuri)
