use super::Strings;
use crate::providers::ProviderId;

pub const STRINGS: Strings = Strings {
    search: "بحث",
    downloads: "التنزيلات",
    settings: "الإعدادات",
    library: "المكتبة",
    nothing_saved_yet: "لا يوجد ما تم حفظه بعد",
    new_playlist_name: "اسم قائمة التشغيل الجديدة",

    queue: "قائمة الانتظار",
    recently_played: "تم تشغيلها مؤخراً",
    now_playing_from: "يتم تشغيله من",
    now_playing: "قيد التشغيل",
    up_next: "التالي",
    no_track_playing: "لا يتم تشغيل أي أغنية",
    no_more_tracks_in_queue: "لا مزيد من الأغاني في قائمة الانتظار",
    no_recently_played_tracks: "لا توجد أغانٍ تم تشغيلها مؤخراً",

    not_playing: "لا شيء قيد التشغيل",
    no_tracks_found: "لم يتم العثور على أغانٍ",

    searching: "جارٍ البحث…",
    loading: "جارٍ التحميل…",
    load_more: "تحميل المزيد",
    no_results_found: "لا توجد نتائج",
    no_recent_searches: "لا عمليات بحث حديثة",
    generating_radio: "جارٍ إنشاء الراديو…",

    not_an_artist_page: "ليست صفحة فنان",
    provided_by: "مقدمة بواسطة",
    retry: "إعادة المحاولة",
    nothing_here: "لا يوجد شيء هنا",
    most_popular_songs: "الأغاني الأكثر شعبية",
    albums: "الألبومات",
    playlists: "قوائم التشغيل",
    fans_also_like: "المعجبون يحبون أيضاً",

    looking_up_lyrics: "جارٍ البحث عن الكلمات…",
    play_a_track_for_lyrics: "شغّل أغنية لعرض كلماتها.",
    lyrics_selectable: "قابلة للتحديد",
    lyrics_synced: "متزامنة",
    lyrics_plain: "عادية",

    playlist_not_found: "لم يتم العثور على قائمة التشغيل",
    add_local: "إضافة محلية",
    downloaded_tracks: "الأغاني التي تم تنزيلها",
    no_downloaded_tracks: "لا توجد أغانٍ تم تنزيلها",

    ctx_play: "تشغيل",
    ctx_play_local: "تشغيل محلي",
    ctx_edit: "تعديل",
    ctx_go_to_artist: "الذهاب إلى الفنان",
    ctx_add_to_playlist: "إضافة إلى قائمة التشغيل",
    ctx_download: "تنزيل",
    ctx_song_radio: "راديو الأغنية",
    ctx_artist_radio: "راديو الفنان",
    ctx_remove_from_queue: "إزالة من قائمة الانتظار",
    ctx_remove_from_playlist: "إزالة من قائمة التشغيل",
    sub_play_via: "تشغيل عبر",
    sub_download_from: "تنزيل من",
    sub_via: "عبر",
    sub_on: "على",
    via_search_suffix: "(بحث)",

    current: "(الحالية)",
    select: "تحديد",
    find: "بحث",
    finding: "جارٍ البحث…",
    found_on: |p| format!("تم العثور عليها على {p}"),
    not_linked: "غير مرتبطة",
    providers: "المزودون",
    no_provider_data: "لا تحتوي هذه الأغنية على بيانات مزود.",
    save: "حفظ",
    cancel: "إلغاء",
    edit_track: "تعديل الأغنية",
    lbl_title: "العنوان",
    lbl_artist: "الفنان",
    ph_track_title: "عنوان الأغنية",
    ph_track_artist: "فنان الأغنية",
    delete: "حذف",
    delete_playlist_q: "حذف قائمة التشغيل؟",
    tracks_wont_be_deleted: "لن يتم حذف الأغاني.",
    lbl_id: "المعرّف",
    lbl_url: "الرابط",
    lbl_artist_id: "معرّف الفنان",
    lbl_duration_secs: "المدة (بالثواني)",
    lbl_thumbnail: "الصورة المصغرة",
    lbl_album: "الألبوم",

    sec_playback: "التشغيل",
    sec_storage: "التخزين",
    sec_history: "السجل",
    sec_appearance: "المظهر",
    sec_dependencies: "الاعتماديات",
    theme_lbl: "السمة",
    language_lbl: "اللغة",
    default_provider_lbl: "مزود البث والتنزيل الافتراضي",
    normalize_volume_lbl: "تطبيع مستوى الصوت بين الأغاني",
    download_dir_lbl: "مجلد التنزيل",
    cache_size_lbl: "أقصى حجم لذاكرة البث المؤقتة (ميغابايت)",
    hist_rows_lbl: "صفوف سجل البحث المعروضة",
    hist_entries_lbl: "مدخلات سجل البحث المحفوظة",
    recent_kept_lbl: "الأغاني التي تم تشغيلها مؤخراً والمحفوظة",
    reset_defaults: "استعادة الافتراضي",

    find_in_list: "بحث في القائمة…",
    scope_songs: "أغاني",
    scope_videos: "فيديوهات",
    scope_artists: "فنانون",
    scope_albums: "ألبومات",
    scope_playlists: "قوائم التشغيل",
    radio_word_song: "أغنية",
    radio_word_artist: "فنان",

    lyrics_copied: "تم نسخ الكلمات إلى الحافظة",
    saved_to_library: "تم الحفظ في المكتبة",
    removed_from_library: "تمت الإزالة من المكتبة",
    no_lyrics_found: "لم يتم العثور على كلمات لهذه الأغنية.",
    select_playlist_drop: "اختر قائمة تشغيل لإفلات الأغاني فيها",
    reordered_playlist: "تم إعادة ترتيب قائمة التشغيل",

    n_saved: |n| format!("{n} محفوظة"),
    n_plays: |n| format!("{} تشغيل", crate::util::format_count(n as u64)),
    search_placeholder: |p| match p {
        ProviderId::YouTube => "ابحث في YouTube Music…".into(),
        ProviderId::Local => "ابحث…".into(),
        other => format!("ابحث في {}…", other.label()),
    },
    added: |n| format!("تمت إضافة {}", ar_tracks(n)),
    added_to: |n, to| format!("تمت إضافة {} إلى {to}", ar_tracks(n)),
    removed_n: |n| format!("تمت إزالة {}", ar_tracks(n)),
    removed_from: |n, from| format!("تمت إزالة {} من {from}", ar_tracks(n)),
    pasted_into: |n, into| format!("تم لصق {} في {into}", ar_tracks(n)),
    saved_title: |t| format!("تم حفظ \"{t}\" في المكتبة"),
    playlist_created: |n| format!("تم إنشاء قائمة التشغيل \"{n}\""),
    added_local: |n| format!("تمت إضافة {} (اختر قائمة تشغيل لتنظيمها)", ar_tracks(n)),
    downloading_n: |n| format!("جارٍ تنزيل {}…", ar_tracks(n)),
    resolving_on: |title, p| format!("جارٍ حل \"{title}\" على {p}…"),
    provider_no_radio: |p| format!("{p} لا يدعم الراديو"),
    generating_radio_for: |w, name| format!("جارٍ إنشاء راديو لـ {w}: {name}…"),
    could_not_find_on: |name, p| format!("تعذّر العثور على \"{name}\" على {p}"),
    opening: |l| format!("جارٍ الفتح: {l}…"),
    opening_artist: |name| format!("جارٍ فتح الفنان: {name}…"),
    download_complete: |path| format!("اكتمل التنزيل! تم الحفظ في {path}"),
    failed_resolve_on: |title, p, e| format!("فشل حل \"{title}\" على {p}: {e}"),
    couldnt_load: |e| format!("تعذّر التحميل: {e}"),
    couldnt_load_lyrics: |e| format!("تعذّر تحميل الكلمات: {e}"),
    search_failed: |e| format!("فشل البحث: {e}"),
    radio_label: |w, name| format!("راديو ({w}): {name}"),
    ctx_add_to_playlist_n: |n| format!("إضافة {} إلى قائمة التشغيل", ar_tracks(n)),
    ctx_download_n: |n| format!("تنزيل {}", ar_tracks(n)),
    ctx_remove_from_queue_n: |n| format!("إزالة {} من قائمة الانتظار", ar_tracks(n)),
    ctx_remove_from_playlist_n: |n| format!("إزالة {} من قائمة التشغيل", ar_tracks(n)),

    import_playlist: "استيراد قائمة تشغيل",
    import_method_native: "أصلية",
    import_method_filelist: "قائمة ملفات",
    import_method_csv: "CSV",
    import_native_hint: "يستورد قوائم التشغيل من ملف playlists.json.",
    import_csv_name_col: "عمود الاسم",
    import_csv_artist_col: "عمود الفنان",
    import_csv_album_col: "عمود الألبوم",
    import_csv_preset: "إعداد مسبق",
    import_csv_preset_default: "افتراضي",
    import_csv_preset_exportify: "Exportify",
    import_csv_exportify_note: "يدعم قوائم تشغيل Spotify المصدّرة من https://exportify.net/.",
    import_pattern_lbl: "نمط اسم الملف",
    import_playlist_name: "اسم قائمة التشغيل",
    import_add_pattern: "إضافة نمط",
    import_select_file: "اختيار ملف",
    import_select_folder: "اختيار مجلد",
    import_pattern_conflict: |a, b| {
        format!(
            "قد يتطابق النمطان \"{a}\" و\"{b}\" مع نفس الملف مع حقول متعارضة. أزل أحدهما أو صححه."
        )
    },
    import_playlists_imported: |n| format!("تم استيراد {}", ar_tracks(n)),
    import_imported_into: |n, into| format!("تم استيراد {} إلى {into}", ar_tracks(n)),
    import_no_tracks: "لم يتم العثور على أغانٍ للاستيراد.",
    import_no_match: "لا يوجد ملف يطابق الأنماط.",
    import_bad_file: "تعذّرت قراءة الملف المحدد.",

    deps_title: "اعتماديات مفقودة",
    deps_intro: "بعض الأدوات التي يستخدمها التطبيق مفقودة. اختر ما تريد تثبيته أو تجاهل للمتابعة (قد لا يعمل البحث والبث حتى تتوفر).",
    deps_install: "تثبيت",
    deps_install_selected: "تثبيت المحدد",
    deps_discard: "تجاهل",
    deps_installing: "جارٍ التثبيت…",
    deps_installed: "تم التثبيت",
    deps_failed: "فشل",
    deps_installed_toast: "تم تثبيت الاعتماديات المحددة.",
    deps_yt_dlp_desc: "يبث ويُنزّل الصوت (مطلوب)",
    deps_ytmusicapi_desc: "بحث YouTube Music (اختياري، يعود إلى yt-dlp)",
    deps_python3_desc: "مطلوب من قِبل ytmusicapi (بحث YouTube Music)",
    deps_python3_manual: "ثبّت Python 3 يدوياً ثم أعد تشغيل التطبيق",
    deps_ytmusicapi_requires_python: "يتطلب Python 3 (ثبّته أولاً)",
    deps_play_requires_yt_dlp: "التشغيل يتطلب yt-dlp — ثبّته من نافذة البدء لتشغيل هذا.",
    deps_source_not_playable: "لا يمكن تشغيل هذا المصدر.",
    deps_found_section_title: "موجودة في نظامك",
    deps_found_section_intro: "هذه متاحة بالفعل. حدد أيّاً منها لتثبيت نسخة يديرها التطبيق (نسخة التطبيق لها الأولوية على نسخة النظام).",
    deps_found_on_system: "موجودة في نظامك",
    deps_managed_by_app: "مثبّتة (يديرها التطبيق)",
    deps_not_installed: "غير مثبّتة",
    deps_delete: "حذف",
    deps_deleting: "جارٍ الحذف…",
    deps_deleted: "تم الحذف",
    deps_delete_failed: "فشل الحذف",

    sec_updates: "التحديثات",
    check_for_updates: "تحقق من التحديثات",
    checking_for_updates: "جارٍ التحقق من التحديثات…",
    up_to_date: "محدث",
    update_available: |v| format!("الإصدار {v} متاح"),
    update_now: "تحديث الآن",
    updating: "جارٍ التحديث…",
    update_applied: |v| format!("تم التحديث إلى {v}. جارٍ إعادة التشغيل…"),
    package_managed: "لا يمكن التحديث التلقائي (عدم وجود إذن بالكتابة في دليل التطبيق). إذا تم تثبيته عبر مدير حزم، استخدمه للتحديث.",
    update_failed: |e| format!("فشل التحديث: {e}"),
};

fn ar_tracks(n: usize) -> String {
    let form = if n == 0 {
        0
    } else if n == 2 {
        2
    } else if (3..=10).contains(&n) {
        3
    } else if (11..=99).contains(&n) {
        4
    } else {
        let m = n % 100;
        if m == 0 {
            4
        } else if m == 2 {
            2
        } else if (3..=10).contains(&m) {
            3
        } else {
            4
        }
    };
    match form {
        0 => String::from("0 أغنية"),
        2 => format!("{n} أغنيتان"),
        3 => format!("{n} أغاني"),
        _ => format!("{n} أغنية"),
    }
}
