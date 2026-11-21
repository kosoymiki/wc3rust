# runtime consumer grep evidence

## BLP
crates/wc3-game/src/assets.rs:16:use wc3_formats::blp::BlpImage;
crates/wc3-game/src/assets.rs:1777:fn resize_to(image: &BlpImage, width: u32, height: u32) -> TextureImage {
crates/wc3-re/src/blp.rs:42:use wc3_formats::blp::{BlpImage, MipLevel};
crates/wc3-re/src/blp.rs:93:    pub image: BlpImage,
crates/wc3-re/src/blp.rs:225:    let image = wc3_formats::blp::decode(bytes)?;

## MDX
crates/wc3-re/src/mdx.rs:27:use wc3_formats::mdx::{Layer, Material, MdxModel, Texture};
crates/wc3-re/src/mdx.rs:43:/// The byte walker is owned by `wc3-formats::mdx::MdxModel::chunks`; this
crates/wc3-re/src/mdx.rs:47:    let list = MdxModel::chunks(bytes)?;
crates/wc3-re/src/mdx.rs:80:    pub fn from_model(model: &MdxModel, chunks: &[MdxChunk]) -> Self {
crates/wc3-re/src/mdx.rs:113:pub fn parse(bytes: &[u8]) -> Result<MdxModel> {
crates/wc3-re/src/mdx.rs:114:    Ok(wc3_formats::mdx::MdxModel::parse(bytes)?)
crates/wc3-re/src/mdx.rs:121:    summary.node_objects = MdxModel::node_object_count(bytes)?;
crates/wc3-re/src/mdx.rs:126:pub fn textures(model: &MdxModel) -> Vec<&Texture> {

## SLK/TXT data
crates/wc3-re/src/txt.rs:48:    pub fn parse(path: impl Into<String>, bytes: &[u8]) -> Result<Self> {
crates/wc3-re/src/txt.rs:65:        let parsed = wc3_formats::gamedata::TextTable::parse(path.clone(), bytes)
crates/wc3-re/src/txt.rs:199:        let table = TextTable::parse("war3skins.txt", &bytes).expect("parse");
crates/wc3-re/src/txt.rs:210:        let table = TextTable::parse("cp1252", b"[S]\nk=caf\x96\n").expect("parse");
crates/wc3-re/src/txt.rs:216:        let table = TextTable::parse("x", b"[S]\nk=a=b\n").unwrap();
crates/wc3-re/src/slk.rs:26:    pub fn parse(name: impl Into<String>, bytes: &[u8]) -> Result<Self> {
crates/wc3-re/src/slk.rs:31:        let table = wc3_formats::slk::SlkTable::parse(bytes)
crates/wc3-re/src/slk.rs:118:        let sheet = Sheet::parse("UnitData", &bytes).expect("parse");

## audio
crates/wc3-game/src/assets.rs:14:    start_game_audio, GameAudioWiring, SharedAssetDb, WiredGameAudio,
crates/wc3-game/src/assets.rs:252:    pub audio: Option<WiredGameAudio>,
crates/wc3-game/src/assets.rs:603:            let (audio, wiring) = start_game_audio(shared.clone());
crates/wc3-game/src/assets.rs:1260:fn log_audio_wiring(wiring: &GameAudioWiring) {
crates/wc3-game/src/assets.rs:1266:    pub fn audio_mut(&mut self) -> Option<&mut WiredGameAudio> {
crates/wc3-game/src/assets.rs:1272:    /// The facade ([`WiredGameAudio`]) is the shared-database instantiation of
crates/wc3-game/src/assets.rs:1273:    /// `wc3_audio::game_audio::GameAudio`; HUD/QA controls that need the raw
crates/wc3-game/src/assets.rs:1277:    ) -> Option<&mut wc3_audio::game_audio::GameAudio<SharedAssetDb>> {

## maps/JASS
crates/wc3-re/src/mapfiles.rs:67:pub struct MapBundle {
crates/wc3-re/src/mapfiles.rs:86:impl MapBundle {
crates/wc3-re/src/mapfiles.rs:91:        let archive = MpqArchive::open_at(&path, base)?;
crates/wc3-re/src/mapfiles.rs:218:    pub fn from_bundle(bundle: &MapBundle) -> Self {
crates/wc3-re/src/mapfiles.rs:268:        let bundle = MapBundle::open(&path).expect("open map");
crates/wc3-re/src/engine.rs:34:use crate::mapfiles::MapBundle;
crates/wc3-re/src/engine.rs:119:    pub map: Option<MapBundle>,
crates/wc3-re/src/engine.rs:131:    /// Map placement → JASS world spawn audit from the last `enter_map`.

## FDF/fonts
crates/wc3-formats/src/fdf.rs:504:pub struct FdfDocument {
crates/wc3-formats/src/fdf.rs:511:impl FdfDocument {
crates/wc3-formats/src/fdf.rs:522:        let mut document = FdfDocument {
crates/wc3-formats/src/fdf.rs:710:    pub fn from_document(document: &FdfDocument) -> Self {
crates/wc3-formats/src/fdf.rs:716:    pub fn from_documents<'a>(documents: impl IntoIterator<Item = &'a FdfDocument>) -> Self {
crates/wc3-formats/src/fdf.rs:725:        Ok(Self::from_document(&FdfDocument::parse_named(text, name)?))
crates/wc3-formats/src/fdf.rs:728:    pub fn merge_document(&mut self, document: &FdfDocument) -> usize {
crates/wc3-formats/src/fdf.rs:737:    pub fn parse_string_list(&mut self, document: &FdfDocument) -> usize {
