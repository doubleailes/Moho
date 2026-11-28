// hydradump: the C++ side of the Hydra parity check (README.md).
//
// Builds UsdImaging's scene indices over a stage (UsdImagingCreateSceneIndices,
// draw mode off), merges them as a render index does, applies light linking
// and dependency forwarding over the merge, and prints, in the format the
// Rust side (crates/moho-parity) shares:
//
// - each USD prototype and its smallest instance (`proto`);
// - every prim and its type (`prim`), and in `read` mode every data source
//   under every locator (`ds`), with values as TfStringify writes them;
// - for each line of the edit script, the line (`edit`) and the notices it
//   gave (`added`, `removed`, `dirtied`, `renamed`);
// - in `read` mode, the whole index again after the edits (`final`).
//
//   hydradump <read|none> <scene> [edits]

#include <pxr/base/tf/stringUtils.h>
#include <pxr/base/vt/value.h>
#include <pxr/imaging/hd/dataSource.h>
#include <pxr/imaging/hd/dependencyForwardingSceneIndex.h>
#include <pxr/imaging/hd/mergingSceneIndex.h>
#include <pxr/imaging/hd/primOriginSchema.h>
#include <pxr/imaging/hd/sceneIndexObserver.h>
#include <pxr/imaging/hdsi/lightLinkingSceneIndex.h>
#include <pxr/usd/sdf/attributeSpec.h>
#include <pxr/usd/sdf/layer.h>
#include <pxr/usd/sdf/schema.h>
#include <pxr/usd/usd/attribute.h>
#include <pxr/usd/usd/prim.h>
#include <pxr/usd/usd/relationship.h>
#include <pxr/usd/usd/stage.h>
#include <pxr/usdImaging/usdImaging/sceneIndices.h>
#include <pxr/usdImaging/usdImaging/stageSceneIndex.h>

#include <algorithm>
#include <fstream>
#include <iostream>
#include <sstream>
#include <string>
#include <vector>

PXR_NAMESPACE_USING_DIRECTIVE

namespace {

// The longest value text written whole.
constexpr size_t LIMIT = size_t(1) << 20;

// `text` on one line, cut after LIMIT bytes (with ` ...`), as mohodump cuts it.
std::string OneLine(const std::string &text)
{
    std::string line = TfStringReplace(text, "\n", "\\n");
    return line.size() > LIMIT ? line.substr(0, LIMIT) + " ..." : line;
}

// The value's text: TfStringify's, but for the types VtValue cannot stream,
// written as openusd writes the values it holds for them (a path vector
// `[ a b ]`, a token or string vector `[a, b]`, a prim origin its path), and
// with no address for any other.
std::string Text(const VtValue &value)
{
    if (value.IsHolding<SdfPathVector>()) {
        std::string text = "[ ";
        for (const SdfPath &path : value.UncheckedGet<SdfPathVector>()) {
            text += path.GetString() + " ";
        }
        return text + "]";
    }
    if (value.IsHolding<TfTokenVector>()) {
        return TfStringify(VtArray<TfToken>(value.UncheckedGet<TfTokenVector>().begin(),
                                            value.UncheckedGet<TfTokenVector>().end()));
    }
    if (value.IsHolding<std::vector<std::string>>()) {
        const auto &strings = value.UncheckedGet<std::vector<std::string>>();
        return TfStringify(VtArray<std::string>(strings.begin(), strings.end()));
    }
    if (value.IsHolding<HdPrimOriginSchema::OriginPath>()) {
        return value.UncheckedGet<HdPrimOriginSchema::OriginPath>().GetPath().GetString();
    }
    std::string text = TfStringify(value);
    if (TfStringStartsWith(text, "<'")) {
        size_t at = text.find("' @ 0x");
        if (at != std::string::npos) {
            return text.substr(0, at + 1) + ">";
        }
    }
    return text;
}

std::string Join(const std::string &locator, const std::string &name)
{
    return locator.empty() ? name : locator + "/" + name;
}

void Source(std::ostream &out, const SdfPath &path, const std::string &locator,
            const HdDataSourceBaseHandle &data)
{
    if (!data) {
        out << "ds " << path << " " << locator << " =null\n";
    } else if (auto container = HdContainerDataSource::Cast(data)) {
        TfTokenVector names = container->GetNames();
        std::vector<std::string> sorted;
        for (const TfToken &name : names) {
            sorted.push_back(name.GetString());
        }
        std::sort(sorted.begin(), sorted.end());
        for (const std::string &name : sorted) {
            Source(out, path, Join(locator, name), container->Get(TfToken(name)));
        }
    } else if (auto vector = HdVectorDataSource::Cast(data)) {
        size_t n = vector->GetNumElements();
        for (size_t i = 0; i < n; ++i) {
            Source(out, path, Join(locator, "[" + std::to_string(i) + "]"), vector->GetElement(i));
        }
    } else if (auto sampled = HdSampledDataSource::Cast(data)) {
        VtValue value = sampled->GetValue(0.0f);
        if (value.IsEmpty()) {
            out << "ds " << path << " " << locator << " =none\n";
        } else {
            out << "ds " << path << " " << locator << " = " << OneLine(Text(value)) << "\n";
        }
    } else if (HdBlockDataSource::Cast(data)) {
        out << "ds " << path << " " << locator << " =block\n";
    } else {
        out << "ds " << path << " " << locator << " =other\n";
    }
}

void Dump(std::ostream &out, const HdSceneIndexBaseRefPtr &index, bool read)
{
    std::vector<SdfPath> stack{SdfPath::AbsoluteRootPath()};
    while (!stack.empty()) {
        SdfPath path = stack.back();
        stack.pop_back();
        HdSceneIndexPrim prim = index->GetPrim(path);
        out << "prim " << path << " " << prim.primType << "\n";
        if (read && prim.dataSource) {
            Source(out, path, "", prim.dataSource);
        }
        SdfPathVector children = index->GetChildPrimPaths(path);
        stack.insert(stack.end(), children.rbegin(), children.rend());
    }
}

void Prototypes(std::ostream &out, const UsdStageRefPtr &stage)
{
    for (const UsdPrim &prototype : stage->GetPrototypes()) {
        std::string smallest;
        for (const UsdPrim &instance : prototype.GetInstances()) {
            const std::string &text = instance.GetPath().GetString();
            if (smallest.empty() || text < smallest) {
                smallest = text;
            }
        }
        out << "proto " << prototype.GetPath() << " " << smallest << "\n";
    }
}

class Recorder : public HdSceneIndexObserver
{
public:
    std::vector<std::string> lines;

    void PrimsAdded(const HdSceneIndexBase &, const AddedPrimEntries &entries) override
    {
        for (const auto &e : entries) {
            lines.push_back("added " + e.primPath.GetString() + " " + e.primType.GetString());
        }
    }

    void PrimsRemoved(const HdSceneIndexBase &, const RemovedPrimEntries &entries) override
    {
        for (const auto &e : entries) {
            lines.push_back("removed " + e.primPath.GetString());
        }
    }

    void PrimsDirtied(const HdSceneIndexBase &, const DirtiedPrimEntries &entries) override
    {
        for (const auto &e : entries) {
            std::vector<std::string> locators;
            for (const HdDataSourceLocator &locator : e.dirtyLocators) {
                locators.push_back(locator.GetString());
            }
            std::sort(locators.begin(), locators.end());
            lines.push_back("dirtied " + e.primPath.GetString() + " " + TfStringJoin(locators, " "));
        }
    }

    void PrimsRenamed(const HdSceneIndexBase &, const RenamedPrimEntries &entries) override
    {
        for (const auto &e : entries) {
            lines.push_back("renamed " + e.oldPrimPath.GetString() + " " + e.newPrimPath.GetString());
        }
    }
};

// The value `text` holds as an attribute of type `type`, read through a
// one-attribute usda layer so that it parses as a layer's would.
VtValue ParseValue(const std::string &type, const std::string &text)
{
    SdfLayerRefPtr layer = SdfLayer::CreateAnonymous(".usda");
    if (!layer->ImportFromString("#usda 1.0\ndef \"X\"\n{\n    " + type + " a = " + text + "\n}\n")) {
        throw std::runtime_error("cannot parse " + text);
    }
    SdfAttributeSpecHandle spec = layer->GetAttributeAtPath(SdfPath("/X.a"));
    if (!spec) {
        throw std::runtime_error("no value parsed");
    }
    return spec->GetDefaultValue();
}

void Apply(const UsdStageRefPtr &stage, const UsdImagingStageSceneIndexRefPtr &index, const std::string &line)
{
    std::istringstream words(line);
    std::string op, target;
    words >> op >> target;
    if (op == "set") {
        std::string type;
        words >> type;
        std::string text;
        std::getline(words, text);
        VtValue value = ParseValue(type, TfStringTrim(text));
        SdfPath path(target);
        UsdPrim prim = stage->GetPrimAtPath(path.GetPrimPath());
        if (!prim) {
            throw std::runtime_error("no prim " + path.GetPrimPath().GetString());
        }
        UsdAttribute attribute = prim.GetAttribute(path.GetNameToken());
        if (!attribute) {
            attribute = prim.CreateAttribute(path.GetNameToken(), SdfSchema::GetInstance().FindType(type));
        }
        if (!attribute.Set(value)) {
            throw std::runtime_error("set failed");
        }
    } else if (op == "define") {
        std::string type;
        words >> type;
        stage->DefinePrim(SdfPath(target), TfToken(type));
    } else if (op == "remove") {
        if (!stage->RemovePrim(SdfPath(target))) {
            throw std::runtime_error("remove failed");
        }
    } else if (op == "instanceable") {
        std::string on;
        words >> on;
        UsdPrim prim = stage->GetPrimAtPath(SdfPath(target));
        if (!prim || !prim.SetInstanceable(on == "true")) {
            throw std::runtime_error("instanceable failed");
        }
    } else if (op == "targets") {
        SdfPathVector targets;
        std::string word;
        while (words >> word) {
            targets.push_back(SdfPath(word));
        }
        SdfPath path(target);
        UsdPrim prim = stage->GetPrimAtPath(path.GetPrimPath());
        if (!prim || !prim.CreateRelationship(path.GetNameToken()).SetTargets(targets)) {
            throw std::runtime_error("targets failed");
        }
    } else if (op == "time") {
        index->SetTime(UsdTimeCode(std::stod(target)));
    } else {
        throw std::runtime_error("unknown edit " + op);
    }
}

} // namespace

int main(int argc, char **argv)
{
    if (argc < 3) {
        std::cerr << "usage: hydradump <read|none> <scene> [edits]\n";
        return 1;
    }
    bool read = std::string(argv[1]) == "read";
    std::ostream &out = std::cout;
    UsdStageRefPtr stage = UsdStage::Open(argv[2]);
    if (!stage) {
        out << "error cannot open " << argv[2] << "\nend\n";
        return 1;
    }
    UsdImagingCreateSceneIndicesInfo info;
    info.stage = stage;
    info.addDrawModeSceneIndex = false;
    UsdImagingSceneIndices indices = UsdImagingCreateSceneIndices(info);
    // As a render index: the filters over the merge first, then the scene
    // added to the merge, so that they see every prim added.
    HdMergingSceneIndexRefPtr merging = HdMergingSceneIndex::New();
    HdSceneIndexBaseRefPtr linking = HdsiLightLinkingSceneIndex::New(merging, nullptr);
    HdSceneIndexBaseRefPtr terminal = HdDependencyForwardingSceneIndex::New(linking);
    merging->AddInputScene(indices.finalSceneIndex, SdfPath::AbsoluteRootPath());
    indices.stageSceneIndex->ApplyPendingUpdates();

    out << "section static\n";
    Prototypes(out, stage);
    Dump(out, terminal, read);

    Recorder recorder;
    terminal->AddObserver(HdSceneIndexObserverPtr(&recorder));
    if (argc > 3) {
        std::ifstream edits(argv[3]);
        std::string line;
        for (int n = 0; std::getline(edits, line); ++n) {
            line = TfStringTrim(line);
            if (line.empty() || line[0] == '#') {
                continue;
            }
            out << "edit " << n << " " << line << "\n";
            try {
                Apply(stage, indices.stageSceneIndex, line);
            } catch (const std::exception &error) {
                out << "editerror " << OneLine(error.what()) << "\n";
            }
            indices.stageSceneIndex->ApplyPendingUpdates();
            for (const std::string &notice : recorder.lines) {
                out << notice << "\n";
            }
            recorder.lines.clear();
            Prototypes(out, stage);
        }
    }
    if (read) {
        out << "section final\n";
        Prototypes(out, stage);
        Dump(out, terminal, true);
    }
    terminal->RemoveObserver(HdSceneIndexObserverPtr(&recorder));
    out << "end\n";
    return 0;
}
